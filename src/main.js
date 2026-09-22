import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";

const ROWS = [
  ["claude", "Claude Code", "#d97757"],
  ["codex", "Codex", "#10a37f"],
  ["gemini", "Gemini", "#4285f4"],
];
const fmt = new Intl.NumberFormat("en", { notation: "compact", maximumFractionDigits: 1 });
const list = document.getElementById("list");
let last = null;
let showUsed = false;

// % of the window still available; a window whose reset time has passed is full again.
const left = (l) => (Date.now() / 1000 >= l.resets_at && l.resets_at > 0 ? 100 : Math.max(0, 100 - l.used_percent));

function until(ts) {
  const s = ts - Date.now() / 1000;
  if (!ts || s <= 0) return "";
  const d = Math.floor(s / 86400), h = Math.floor((s % 86400) / 3600), m = Math.floor((s % 3600) / 60);
  return d ? `${d}d ${h}h` : h ? `${h}h ${m}m` : `${m}m`;
}

function bar(label, l) {
  if (!l) return "";
  const p = left(l);
  const shown = showUsed ? 100 - p : p;
  const color = p > 50 ? "var(--ok)" : p > 20 ? "var(--warn)" : "var(--bad)"; // colour always tracks what's left
  const reset = until(l.resets_at);
  return `<div class="flex items-center gap-2.5 text-[0.6875rem]">
    <span class="w-7 text-ink/70">${label}</span>
    <div class="lg-track h-2 flex-1 p-px">
      <div class="lg-fill h-full" style="width:${shown}%;--c:${color}"></div>
    </div>
    <span class="w-[6rem] whitespace-nowrap text-right tabular-nums"><span class="font-medium">${Math.round(shown)}%</span><span class="text-ink/70"> ${showUsed ? "used" : "left"}</span>${reset ? `<span class="text-ink/70"> · ${reset}</span>` : ""}</span>
  </div>`;
}

function body(r) {
  if (r.status) return `<div class="text-[0.6875rem] text-ink/70">${r.status}</div>`;
  if (r.five_hour || r.weekly) return bar("5h", r.five_hour) + bar("wk", r.weekly);
  return `<div class="text-[0.6875rem] tabular-nums text-ink/75">${fmt.format(r.tokens_5h ?? 0)} <span class="text-ink/70">in 5h</span> · ${fmt.format(r.tokens_24h ?? 0)} <span class="text-ink/70">in 24h</span></div>`;
}

// Scale everything together: measure the content at 16px/rem, then pick the largest
// rem at which it fits both the window's height and a 270px-wide design width.
const BASE_W = 270;
const fitBox = document.getElementById("fit");
function fit() {
  const root = document.documentElement.style;
  root.fontSize = "16px";
  if (!innerWidth || !innerHeight) return; // minimized / hidden: keep the base size
  const scale = Math.min(innerWidth / BASE_W, innerHeight / fitBox.offsetHeight);
  root.fontSize = `${16 * scale}px`;
  drawBackdrop(); // tiles moved or resized
}
addEventListener("resize", fit);

// ---------- Real blur ----------
// The backend sends a tiny copy of the screen behind the widget (~5x/s, only when it changes).
// Each [data-glass] tile gets that image blurred and brightened like iOS glass, plus just enough
// darkening under each card for its white text to stay readable.
const canvas = document.getElementById("backdrop");
const small = document.createElement("canvas");
let frame = null; // { w, h, px }
let realBlur = false;
let glass = 0.55;

/** Mean brightness (0-1, sRGB) of the frame inside a rectangle given in window fractions. */
function brightness(fx0, fy0, fx1, fy1) {
  const { w, h, px } = frame;
  const x0 = Math.max(0, Math.floor(fx0 * w)), x1 = Math.min(w, Math.ceil(fx1 * w));
  const y0 = Math.max(0, Math.floor(fy0 * h)), y1 = Math.min(h, Math.ceil(fy1 * h));
  let sum = 0, n = 0;
  for (let y = y0; y < y1; y++)
    for (let x = x0; x < x1; x++) {
      const i = (y * w + x) * 4;
      sum += 0.2126 * px[i] + 0.7152 * px[i + 1] + 0.0722 * px[i + 2];
      n++;
    }
  return n ? sum / n / 255 : 0.5;
}

function drawBackdrop() {
  const root = document.documentElement;
  root.classList.toggle("real-blur", realBlur && !!frame);
  if (!realBlur || !frame || !innerWidth) return;
  const { w, h, px } = frame;
  small.width = w;
  small.height = h;
  small.getContext("2d").putImageData(new ImageData(new Uint8ClampedArray(px), w, h), 0, 0);

  const k = devicePixelRatio / 2; // half resolution is plenty under a blur
  canvas.width = Math.round(innerWidth * k);
  canvas.height = Math.round(innerHeight * k);
  const ctx = canvas.getContext("2d");
  const shape = (el) => {
    const r = el.getBoundingClientRect();
    const radius = parseFloat(getComputedStyle(el).borderTopLeftRadius) || 0;
    ctx.beginPath();
    ctx.roundRect(r.x * k, r.y * k, r.width * k, r.height * k, radius * k);
    return r;
  };

  // 1. The whole panel is glass: the blurred, brightened screen behind it, plus a base smoke.
  ctx.save();
  shape(document.querySelector("main"));
  ctx.clip();
  const o = 40 * k; // overscan so the blur does not fade in from the edges
  ctx.filter = `blur(${22 * k}px) saturate(1.8) brightness(1.08)`;
  ctx.drawImage(small, -o, -o, canvas.width + 2 * o, canvas.height + 2 * o);
  ctx.filter = "none";
  const base = glass * 0.2; // Glass opacity adds a little smoke
  ctx.fillStyle = `rgba(0,0,0,${base})`;
  ctx.fillRect(0, 0, canvas.width, canvas.height);
  ctx.restore();

  // 2. Contrast guard under each card: darken only as much as its white text needs there
  //    (never a white tint). Brightness under text must stay <= 0.45, about 5:1 contrast.
  for (const el of document.querySelectorAll("[data-glass]")) {
    const r = el.getBoundingClientRect();
    const b = Math.min(1, brightness(r.x / innerWidth, r.y / innerHeight, r.right / innerWidth, r.bottom / innerHeight) * 1.08) * (1 - base);
    const a = 1 - 0.45 / Math.max(b, 0.01);
    if (a <= 0) continue;
    shape(el);
    ctx.fillStyle = `rgba(0,0,0,${a})`;
    ctx.fill();
  }
}

listen("backdrop", (e) => {
  frame = e.payload;
  requestAnimationFrame(drawBackdrop);
});

function render(snap) {
  last = snap;
  const rows = ROWS.filter(([key]) => snap[key].connected); // tools that are off or not found stay hidden
  list.innerHTML = rows.length
    ? rows.map(([key, name, color]) => `<li data-glass class="lg-card flex flex-col gap-1.5 px-3 py-2.5">
        <div class="flex items-center gap-1.5 text-xs font-semibold">
          <span class="size-1.5 rounded-full" style="background:${color};box-shadow:0 0 6px ${color}"></span>${name}
        </div>
        ${body(snap[key])}
      </li>`).join("")
    : `<li data-glass class="lg-card flex flex-col items-start gap-2 px-3 py-2.5 text-[0.6875rem] text-ink/75">
        No tools connected yet.
        <button data-open-settings class="lg-btn px-3 py-0.5 text-ink">Open settings</button>
      </li>`;
  fit();
}

function applySettings(s) {
  showUsed = s.show_used;
  document.documentElement.style.setProperty("--glass", s.glass_opacity / 100); // tints the glass only; text stays sharp
  glass = s.glass_opacity / 100;
  realBlur = s.blur;
  if (!realBlur) frame = null; // fall back to the frosted tiles
  if (last) render(last);
  else drawBackdrop();
}

const openSettings = () => invoke("open_settings");
document.addEventListener("click", (e) => e.target.closest("[data-open-settings]") && openSettings());

invoke("get_settings").then(applySettings);
invoke("get_usage").then(render); // pulls the state on load, so no emits are missed before the listener is ready
listen("usage", (e) => render(e.payload));
listen("settings", (e) => applySettings(e.payload));
setInterval(() => last && render(last), 30_000); // keep reset countdowns ticking
document.getElementById("close").onclick = () => getCurrentWindow().close();
document.getElementById("grip").onmousedown = (e) => {
  e.preventDefault();
  getCurrentWindow().startResizeDragging("SouthEast");
};
