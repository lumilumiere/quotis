// Sets data-theme="dark" | "light" on <html>. "system" follows the OS and keeps following it.
const media = matchMedia("(prefers-color-scheme: dark)");
let choice = "system";

const resolve = () => (choice === "system" ? (media.matches ? "dark" : "light") : choice);
media.addEventListener("change", () => (document.documentElement.dataset.theme = resolve()));

export function applyTheme(theme) {
  choice = ["dark", "light"].includes(theme) ? theme : "system";
  document.documentElement.dataset.theme = resolve();
}
