import { invoke } from "@tauri-apps/api/core";
import { emit, listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { connectionBadge, newestUsage } from "./connection.js";

const $ = (id) => document.getElementById(id);
const root = document.documentElement;
let settings;
let providers = [];
let receivedUsage;

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

const option = (value, text) => el("option", { value, text });
const cfg = (p) => settings.providers[p.id] ?? { enabled: p.default_enabled, dir: "" };

/** The provider list comes from the backend registry, so a new adapter appears here by itself. */
function buildTools() {
  $("tools").replaceChildren(
    ...providers.map((p) => {
      const where = p.endpoint ? `Contacts ${p.endpoint}${p.documented ? "" : " (undocumented endpoint, may change)"}.` : "Local files only, no network.";
      return el(
        "div",
        { class: "space-y-2 px-3.5 py-3" },
        el(
          "label",
          { class: "flex items-center justify-between gap-3" },
          el("span", {}, el("span", { text: p.name }), el("span", { id: `st-${p.id}`, class: "ml-2 text-[11px]" })),
          el("input", { type: "checkbox", class: "lg-switch", "data-id": p.id, "data-field": "enabled" }),
        ),
        !p.key_env &&
          el("input", {
            class: "lg-field w-full px-2.5 py-1.5 font-mono text-[11px] placeholder:font-sans",
            "data-id": p.id,
            "data-field": "dir",
            spellcheck: "false",
            placeholder: "Folder (auto-detect)",
            "aria-label": `${p.name} folder`,
          }),
        el("p", { class: "text-[11px] leading-relaxed text-ink/60", text: `${p.hint} ${where}` }),
        !p.key_env && el("p", { id: `path-${p.id}`, class: "truncate font-mono text-[10px] text-ink/60" }),
      );
    }),
  );
  $("pill_provider").replaceChildren(option("", "Automatic"), ...providers.map((p) => option(p.id, p.name)));
}

function showStatus(list) {
  providers = list;
  for (const p of providers) {
    const badge = $(`st-${p.id}`);
    if (!badge) continue;
    const state = connectionBadge(p, receivedUsage?.providers.find((reading) => reading.id === p.id));
    badge.textContent = state.label;
    badge.className = `ml-2 text-[11px] ${state.tone === "quiet" ? "text-ink/60" : state.tone === "ok" ? "text-ok" : "text-warn"}`;
    const path = $(`path-${p.id}`);
    if (path) path.textContent = path.title = p.path;
  }
}

const FIELDS = ["mode", "display", "pill_provider", "theme", "auto_collapse_secs", "warn_below", "refresh_min", "opacity"];
const TOGGLES = ["hover_expand", "glass", "always_on_top", "autostart"];
const LIMITS = { auto_collapse_secs: [0, 600], warn_below: [0, 90], refresh_min: [1, 60], opacity: [20, 100] };

function applyTheme() {
  const dark = matchMedia("(prefers-color-scheme: dark)").matches;
  root.dataset.theme = settings.theme === "auto" ? (dark ? "dark" : "light") : settings.theme;
}

function fill() {
  for (const input of document.querySelectorAll("[data-id]")) {
    const v = cfg(providers.find((p) => p.id === input.dataset.id))[input.dataset.field];
    if (input.type === "checkbox") input.checked = v;
    else input.value = v;
  }
  for (const id of FIELDS) $(id).value = settings[id];
  for (const id of TOGGLES) $(id).checked = settings[id];
  $("show_used").value = settings.show_used ? "used" : "left";
  $("opacity_out").value = `${settings.opacity}%`;
  applyTheme();
}

function read() {
  for (const input of document.querySelectorAll("[data-id]")) {
    const id = input.dataset.id;
    settings.providers[id] = { ...cfg(providers.find((p) => p.id === id)), ...settings.providers[id] };
    settings.providers[id][input.dataset.field] = input.type === "checkbox" ? input.checked : input.value.trim();
  }
  for (const id of FIELDS) {
    const limit = LIMITS[id];
    settings[id] = limit ? Math.min(limit[1], Math.max(limit[0], Math.round(Number($(id).value) || 0))) : $(id).value;
  }
  for (const id of TOGGLES) settings[id] = $(id).checked;
  settings.show_used = $("show_used").value === "used";
}

let flash;
let saving = Promise.resolve();
function save() {
  read();
  fill();
  const requested = structuredClone(settings);
  saving = saving.then(async () => {
    try {
      showStatus(await invoke("save_settings", { settings:requested }));
      $("saved").textContent = "Saved";
    } catch (e) {
      $("saved").textContent = `Couldn't save: ${e}`;
    }
    clearTimeout(flash);
    flash = setTimeout(() => ($("saved").textContent = ""), 1500);
  });
  return saving;
}

document.addEventListener("change", save);
// Live preview while dragging the slider; saved when released.
$("opacity").addEventListener("input", () => {
  $("opacity_out").value = `${$("opacity").value}%`;
  emit("settings", { ...settings, opacity: Number($("opacity").value) });
});
$("close").onclick = () => getCurrentWindow().close();
addEventListener("keydown", (e) => e.key === "Escape" && getCurrentWindow().close());

await listen("usage", (event) => {
  receivedUsage = newestUsage(receivedUsage, event.payload);
  if (settings) showStatus(providers);
});
const [s, list, displays, usage] = await Promise.all([invoke("get_settings"), invoke("get_providers"), invoke("list_displays"), invoke("get_usage")]);
receivedUsage = newestUsage(receivedUsage, usage);
settings = s;
providers = list;
buildTools();
$("display").replaceChildren(option("", "Primary"), ...displays.map((d) => option(d, d)));
// A saved screen that is unplugged stays selectable, so the choice survives until it returns.
if (settings.display && !displays.includes(settings.display)) $("display").append(option(settings.display, `${settings.display} (not connected)`));
fill();
showStatus(list);
