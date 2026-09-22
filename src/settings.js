import { invoke } from "@tauri-apps/api/core";
import { emit } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";

const TOOLS = [
  ["claude", "Claude Code", "Sign in to Claude Code with a Pro or Max plan. Shows your exact 5-hour and weekly limits."],
  ["codex", "Codex", "Sign in to the Codex CLI with ChatGPT. Limits appear after your first Codex message."],
  ["gemini", "Gemini CLI", "Shows tokens used in the last 5 hours and 24 hours (Gemini doesn't store its quota locally)."],
];
const $ = (id) => document.getElementById(id);
let settings;

$("tools").innerHTML = TOOLS.map(([key, name, hint]) => `
  <div class="space-y-2 px-3.5 py-3">
    <label class="flex items-center justify-between gap-3">
      <span>${name} <span id="st-${key}" class="ml-1 text-[11px]"></span></span>
      <input type="checkbox" data-key="${key}" data-field="enabled" class="lg-switch" />
    </label>
    <input data-key="${key}" data-field="dir" spellcheck="false" placeholder="Folder (auto-detect)"
      class="lg-field w-full px-2.5 py-1.5 font-mono text-[11px] placeholder:font-sans" />
    <p class="text-[11px] leading-relaxed text-white/45">${hint}</p>
    <p id="path-${key}" class="truncate font-mono text-[10px] text-white/35"></p>
  </div>`).join("");

function showStatus(status) {
  for (const [key] of TOOLS) {
    const s = status[key];
    const on = settings[key].enabled;
    const badge = $(`st-${key}`);
    badge.textContent = !on ? "Off" : s.found ? "Connected" : "Not found";
    badge.className = `ml-1 text-[11px] ${!on ? "text-white/40" : s.found ? "text-[#30d158]" : "text-[#ffd60a]"}`;
    $(`path-${key}`).textContent = s.path; // textContent: paths are user input
    $(`path-${key}`).title = s.path;
  }
}

const setGlass = (pct) => ($("glass_out").value = `${pct}%`); // the widget previews it live

function fill() {
  for (const el of document.querySelectorAll("[data-key]")) {
    const v = settings[el.dataset.key][el.dataset.field];
    el.type === "checkbox" ? (el.checked = v) : (el.value = v);
  }
  $("always_on_top").checked = settings.always_on_top;
  $("show_used").value = settings.show_used ? "used" : "left";
  $("glass_opacity").value = settings.glass_opacity;
  setGlass(settings.glass_opacity);
  $("blur").checked = settings.blur;
  $("refresh").value = settings.claude_refresh_min;
}

let flash;
async function save() {
  for (const el of document.querySelectorAll("[data-key]")) {
    settings[el.dataset.key][el.dataset.field] = el.type === "checkbox" ? el.checked : el.value.trim();
  }
  settings.always_on_top = $("always_on_top").checked;
  settings.show_used = $("show_used").value === "used";
  settings.glass_opacity = Number($("glass_opacity").value);
  settings.blur = $("blur").checked;
  settings.claude_refresh_min = Math.min(60, Math.max(1, Math.round(Number($("refresh").value) || 5)));
  $("refresh").value = settings.claude_refresh_min;
  try {
    showStatus(await invoke("save_settings", { settings }));
    $("saved").textContent = "Saved";
  } catch (e) {
    $("saved").textContent = `Couldn't save: ${e}`;
  }
  clearTimeout(flash);
  flash = setTimeout(() => ($("saved").textContent = ""), 1500);
}

// "change" fires once per edit (on blur/Enter for text, on release for the slider), so each save is one write.
document.addEventListener("change", save);
// While dragging the slider, preview the glass live in both windows without saving.
$("glass_opacity").addEventListener("input", () => {
  const pct = Number($("glass_opacity").value);
  setGlass(pct);
  emit("settings", { ...settings, glass_opacity: pct });
});
$("close").onclick = () => getCurrentWindow().close();

settings = await invoke("get_settings");
fill();
showStatus(await invoke("get_status"));
