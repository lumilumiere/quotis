import { invoke } from "@tauri-apps/api/core";

const TOOLS = [
  ["claude", "Claude Code", "Sign in to Claude Code with a Pro or Max plan. Shows your exact 5-hour and weekly limits."],
  ["codex", "Codex", "Sign in to the Codex CLI with ChatGPT. Limits appear after your first Codex message."],
  ["gemini", "Gemini CLI", "Shows tokens used in the last 5 hours and 24 hours (Gemini doesn't store its quota locally)."],
];
const $ = (id) => document.getElementById(id);
let settings;

$("tools").innerHTML = TOOLS.map(([key, name, hint]) => `
  <div class="space-y-2 rounded-lg border border-white/10 bg-white/5 p-3">
    <label class="flex items-center gap-2 font-medium">
      <input type="checkbox" data-key="${key}" data-field="enabled" class="size-4 accent-emerald-400" />
      ${name}
      <span id="st-${key}" class="ml-auto text-xs font-normal"></span>
    </label>
    <input data-key="${key}" data-field="dir" spellcheck="false" placeholder="Folder (auto-detect)"
      class="w-full rounded-md border border-white/10 bg-black/30 px-2 py-1 font-mono text-xs placeholder:font-sans" />
    <p class="text-xs text-white/50">${hint}</p>
    <p id="path-${key}" class="truncate font-mono text-[11px] text-white/35"></p>
  </div>`).join("");

function showStatus(status) {
  for (const [key] of TOOLS) {
    const s = status[key];
    const on = settings[key].enabled;
    const badge = $(`st-${key}`);
    badge.textContent = !on ? "Off" : s.found ? "● Connected" : "○ Not found";
    badge.className = `ml-auto text-xs font-normal ${!on ? "text-white/40" : s.found ? "text-emerald-400" : "text-amber-400"}`;
    $(`path-${key}`).textContent = s.path; // textContent: paths are user input
    $(`path-${key}`).title = s.path;
  }
}

function fill() {
  for (const el of document.querySelectorAll("[data-key]")) {
    const v = settings[el.dataset.key][el.dataset.field];
    el.type === "checkbox" ? (el.checked = v) : (el.value = v);
  }
  $("always_on_top").checked = settings.always_on_top;
  $("show_used").value = settings.show_used ? "used" : "left";
  $("opacity").value = settings.widget_opacity;
  $("opacity_out").value = `${settings.widget_opacity}%`;
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
  settings.widget_opacity = Number($("opacity").value);
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
$("opacity").addEventListener("input", () => ($("opacity_out").value = `${$("opacity").value}%`));

settings = await invoke("get_settings");
fill();
showStatus(await invoke("get_status"));
