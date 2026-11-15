import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { describe, level, percentLeft, refresh, summarize, visible } from "./format.js";

const $ = (id) => document.getElementById(id);
const root = document.documentElement;
const island = $("island");
const pill = $("pill");
const list = $("list");

const BAR_H = 32;
const OPEN_W = 300;
const MAX_H = 560;

let received = null;
let snap = null;
let settings = {};
let expanded = false;
let openedByHover = false;
let shown = { w: 0, h: 0 };
let resizeSeq = 0;

/** Builds an element. Text always goes through textContent: provider strings are never parsed as HTML. */
function el(tag, attrs = {}, ...children) {
  const n = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (v == null || v === false) continue;
    if (k === "class") n.className = v;
    else if (k === "text") n.textContent = v;
    else n.setAttribute(k, v === true ? "" : v);
  }
  n.append(...children.filter(Boolean));
  return n;
}

const isOpen = () => expanded || settings.mode === "panel";
const duration = () => (matchMedia("(prefers-reduced-motion: reduce)").matches ? 0 : 260);
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

function metricRow(m, now) {
  const d = describe(m, now, settings.show_used);
  const left = percentLeft(m);
  const row = el("div", { class: "metric", "data-freshness": m.freshness, title: m.qualification });
  row.append(el("span", { class: "metric-label", text: m.label }));
  if (left != null) {
    const fill = el("span", { class: "fill", "data-level": level(m, settings.warn_below ?? 20) });
    fill.style.width = `${m.freshness === "expired" ? 0 : settings.show_used ? 100 - left : left}%`;
    row.append(el("span", { class: "track" }, fill));
  }
  row.append(el("span", { class: "metric-text" }, el("b", { text: d.value }), el("span", { text: ` ${d.detail}` })));
  return row;
}

function providerCard(p, now) {
  const head = el("div", { class: "card-head" }, el("span", { class: "card-name", text: p.name }));
  if (p.account) head.append(el("span", { class: "tag", text: p.account }));
  if (p.refreshing) head.append(el("span", { class: "tag", text: "updating" }));
  const card = el("li", { class: "card", "data-status": p.status }, head);
  // Allowances first (as bars), then measured usage and estimates as plain numbers.
  const quotas = p.metrics.filter((m) => m.category === "provider_quota");
  const usage = p.metrics.filter((m) => m.category !== "provider_quota");
  for (const m of quotas) card.append(metricRow(m, now));
  if (usage.length) card.append(el("div", { class: "usage" }, ...usage.map((m) => metricRow(m, now))));
  for (const note of p.notes) card.append(el("p", { class: "note", text: note }));
  if (p.error) card.append(el("p", { class: "note problem", "data-kind": p.error.kind, text: p.error.message }));
  if (!p.metrics.length && !p.notes.length && !p.error) card.append(el("p", { class: "note", text: p.refreshing ? "Checking…" : "Waiting for data" }));
  return card;
}

function render() {
  const now = Date.now() / 1000;
  // `received` is what the backend sent; `snap` is that, judged for this moment.
  snap = refresh(received, now);
  const s = summarize(snap, settings, now);
  island.dataset.state = s.state;
  island.toggleAttribute("data-refreshing", s.refreshing);
  island.toggleAttribute("data-stale", s.stale);
  $("pill-dot").dataset.level = s.state === "none" ? "unknown" : ["offline", "denied", "disconnected", "error"].includes(s.state) ? "bad" : s.level;
  $("pill-name").textContent = s.name;
  $("pill-value").textContent = s.value;
  $("pill-others").replaceChildren(...s.others.map((o) => el("span", { class: "dot mini", "data-level": o.level })));
  pill.title = s.title;
  pill.setAttribute("aria-label", `${s.title}. ${isOpen() ? "Collapse" : "Expand"} details`);
  if ($("live").textContent !== s.title && ["warning", "offline", "denied", "error", "disconnected"].includes(s.state)) $("live").textContent = s.title;

  const rows = visible(snap);
  list.replaceChildren(
    ...(rows.length
      ? rows.map((p) => providerCard(p, now))
      : [el("li", { class: "card empty" }, el("p", { class: "note", text: "No tools connected yet." }), el("button", { type: "button", class: "text-btn", "data-open-settings": true, text: "Open settings" }))]),
  );
  resize();
}

/** Makes the island, and the window around it, the size its content needs. */
async function resize() {
  const open = isOpen();
  // Never taller than the screen can show: the list scrolls inside whatever height is left.
  const roomy = Math.max(BAR_H + 60, Math.min(MAX_H, (screen.availHeight || MAX_H) - 24));
  const target = open ? { w: OPEN_W, h: Math.min(BAR_H + list.scrollHeight, roomy) } : { w: Math.ceil($("pill-content").offsetWidth) + 26, h: BAR_H };
  island.toggleAttribute("data-open", open);
  pill.setAttribute("aria-expanded", String(open));
  list.inert = !open;
  $("actions").inert = !open;
  if (target.w === shown.w && target.h === shown.h) return;

  const seq = ++resizeSeq;
  // The window must be at least as large as the island for the whole animation:
  // grow it before the island grows, shrink it only after the island has shrunk.
  const during = { w: Math.max(shown.w, target.w), h: Math.max(shown.h, target.h) };
  if (during.w !== shown.w || during.h !== shown.h) await invoke("set_size", { width: during.w, height: during.h });
  if (seq !== resizeSeq) return;
  island.style.setProperty("--w", `${target.w}px`);
  island.style.setProperty("--h", `${target.h}px`);
  shown = target;
  if (during.w !== target.w || during.h !== target.h) {
    await sleep(duration());
    if (seq === resizeSeq) await invoke("set_size", { width: target.w, height: target.h });
  }
}

let collapseTimer;
function armAutoCollapse() {
  clearTimeout(collapseTimer);
  const secs = settings.auto_collapse_secs ?? 0;
  if (expanded && secs > 0 && settings.mode !== "panel") collapseTimer = setTimeout(() => setExpanded(false), secs * 1000);
}

function setExpanded(on, byHover = false) {
  if (settings.mode === "panel") on = true;
  if (expanded === on) return;
  expanded = on;
  openedByHover = on && byHover;
  render();
  armAutoCollapse();
}

function applySettings(s) {
  settings = s;
  island.dataset.mode = s.mode;
  root.toggleAttribute("data-glass", !!s.glass);
  root.style.setProperty("--opacity", (s.opacity ?? 80) / 100);
  applyTheme();
  render();
}

const dark = matchMedia("(prefers-color-scheme: dark)");
function applyTheme() {
  root.dataset.theme = settings.theme === "auto" || !settings.theme ? (dark.matches ? "dark" : "light") : settings.theme;
}
dark.addEventListener("change", applyTheme);

// Click toggles; a press that moves becomes a window drag instead (not in notch mode, which is anchored).
let press = null;
let dragged = false;
island.addEventListener("pointerdown", (e) => {
  dragged = false;
  const onControl = e.target.closest("button, a, input") && !e.target.closest("#pill");
  press = e.button === 0 && !onControl && settings.mode !== "notch" ? { x: e.screenX, y: e.screenY } : null;
});
island.addEventListener("pointermove", (e) => {
  armAutoCollapse();
  if (!press || Math.hypot(e.screenX - press.x, e.screenY - press.y) < 5) return;
  press = null;
  dragged = true;
  getCurrentWindow().startDragging();
});
addEventListener("pointerup", () => (press = null));

pill.addEventListener("click", () => {
  if (dragged) return (dragged = false);
  setExpanded(!expanded);
});

let hoverTimer;
island.addEventListener("pointerenter", () => {
  clearTimeout(hoverTimer);
  if (settings.hover_expand && !expanded) hoverTimer = setTimeout(() => setExpanded(true, true), 150);
});
island.addEventListener("pointerleave", () => {
  clearTimeout(hoverTimer);
  if (openedByHover) hoverTimer = setTimeout(() => setExpanded(false), 500);
});

addEventListener("keydown", (e) => {
  armAutoCollapse();
  if (e.key === "Escape" && expanded) {
    setExpanded(false);
    pill.focus();
  }
});

document.addEventListener("click", (e) => e.target.closest("[data-open-settings]") && invoke("open_settings"));
$("open-settings").onclick = () => invoke("open_settings");
$("refresh").onclick = () => invoke("refresh");
$("quit").onclick = () => invoke("quit");

listen("usage", (e) => {
  received = e.payload;
  render();
});
listen("settings", (e) => applySettings(e.payload));
// Countdowns and freshness are relative to now; redraw them even when no data arrives.
setInterval(() => document.visibilityState === "visible" && render(), 30_000);

const [s, usage] = await Promise.all([invoke("get_settings"), invoke("get_usage")]);
received = usage;
applySettings(s);
