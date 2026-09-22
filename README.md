# Quotis

A tiny floating widget that shows how much of your AI coding limits you have left: **Claude Code**, **Codex** and **Gemini CLI**, at a glance.

It sits on your desktop like a sticky note: a minimal, see-through Liquid Glass panel that stays on top and can be dragged and resized.

| Tool | What Quotis shows | Where it comes from |
|---|---|---|
| **Claude Code** | 5-hour and weekly limits: % left + time until reset | Anthropic's usage endpoint (the one Claude Code's `/usage` uses), with your existing Claude Code login |
| **Codex** | 5-hour and weekly limits: % left + time until reset | Codex's local session logs (`~/.codex/sessions`) |
| **Gemini CLI** | Tokens used in the last 5 hours and 24 hours | Gemini CLI's local chat logs (`~/.gemini/tmp`) |

Tools you don't use, or turn off in Settings, simply don't appear.

> Quotis is an independent project. It is not affiliated with or endorsed by Anthropic, OpenAI or Google.

## Install

Download the installer for your system from the [latest release](https://github.com/lumilumiere/quotis/releases/latest):

- **Windows**: `.msi` or `-setup.exe`
- **macOS**: `.dmg` (`aarch64` for Apple Silicon, `x64` for Intel)
- **Linux**: `.AppImage`, `.deb` or `.rpm`

The builds are not code-signed, so your OS will warn you the first time:

- **Windows**: SmartScreen shows "Windows protected your PC". Click **More info → Run anyway**.
- **macOS**: if it says the app is damaged or from an unidentified developer, run `xattr -cr /Applications/Quotis.app` once, then open it again.

## Using it

- **Move**: drag anywhere on the widget.
- **Resize**: drag the grip in the bottom-right corner. Everything scales to fit.
- **Settings** and **Quit**: the gear and × appear when you hover over the widget.

### Settings

- **Connections**: turn each tool on or off. Quotis auto-detects each tool's folder (`~/.claude`, `~/.codex`, `~/.gemini`, and it respects `CLAUDE_CONFIG_DIR` / `CODEX_HOME`). If yours lives elsewhere, type the folder in and the status will show **Connected** once it's found.
- **Appearance**: rich blurred glass with white text and bars. it adapts to whatever is behind it, darkening only where the text needs it. Glass opacity (adds smoke), **real blur**, keep on top, and bars as % left or % used.
- **Refresh**: how often to check Claude's limits (1–60 min). Codex and Gemini update instantly when their logs change.

### Requirements per tool

- **Claude Code**: signed in with a Claude Pro or Max plan (API-key logins don't have 5-hour/weekly limits). If the row says *login expired*, open Claude Code once; it refreshes the login and Quotis picks it up.
- **Codex**: the Codex CLI signed in with ChatGPT. Limits appear after your first Codex message.
- **Gemini CLI**: any Gemini CLI install. Gemini doesn't store its quota locally, so Quotis shows token counts instead.

## Privacy

- Everything is read from files already on your computer. Nothing is uploaded or collected.
- The **only** network request is Claude's limit check: a small request to `api.anthropic.com`, sent with the login Claude Code already stored on your machine. Quotis never modifies or refreshes that login. On macOS, it reads it from the Keychain, which may ask you to allow access. Turn Claude off in Settings to stop the request entirely.
- **Real blur (Windows):** to blur what is behind the widget, Quotis reads a tiny, low-resolution copy of that patch of screen about once a second (and when you move it). It stays in memory and is never saved or sent anywhere. For the ~50 ms of each copy the widget hides itself from capture so it does not blur itself; the rest of the time it shows in screenshots, recordings and screen sharing as normal. (macOS uses the system blur instead.)
- The Claude endpoint is undocumented and may change. If it does, the Claude row shows an error instead of wrong numbers.

## Security

Quotis reads sensitive things (your AI tools' logs, your Claude login, a patch of your screen), so here is exactly what it does with them. Everything below can be checked in [`src-tauri/src/main.rs`](src-tauri/src/main.rs), which is the whole backend.

**What leaves your computer:** one HTTPS request to `api.anthropic.com/api/oauth/usage`, at most once a minute, and only while Claude is enabled. It returns your 5-hour and weekly percentages. There is no telemetry, no analytics, no crash reporting and no update check. Turn **Claude** off in Settings and Quotis makes no network requests at all.

**Your Claude login:** Quotis reads the token Claude Code already stored (`~/.claude/.credentials.json`, or the Keychain on macOS, which asks your permission) and sends it only to `api.anthropic.com` in that one request, over TLS. It never writes, refreshes, copies or logs the token, and never sends it anywhere else. Quotis cannot spend your quota: the endpoint only reports usage.

**Your logs and prompts:** the Codex and Gemini parsers read only the numbers they need (percentages, token counts, timestamps) out of local log files. Your prompts and the tools' replies are never parsed, stored or transmitted.

**The screen capture (Windows, Real blur only):** Quotis copies the small patch of screen behind its own window, 64 pixels wide, a few times a second. The pixels go straight to its own window to be blurred, are never written to disk or sent anywhere, and are dropped on the next frame. It captures nothing outside its own rectangle. Turn **Real blur** off and no capture happens.

**What Quotis writes:** one file, `settings.json`, in your app config folder. Nothing else on your disk is modified.

**Permissions:** the app runs as your normal user and never asks for admin rights. Its UI is only allowed to move, resize and close its own windows, plus the five commands it defines (read usage, read and save settings, open Settings). It has no shell, file-system or HTTP permissions, so a UI bug cannot run commands or read arbitrary files. The UI loads only files bundled inside the app; a Content Security Policy blocks any remote script, and no user or file content is ever rendered as HTML.

**Installers are not code-signed.** Windows SmartScreen and macOS Gatekeeper will warn on first launch. Every release is built by [GitHub Actions](.github/workflows/release.yml) from the tagged commit, so you can see the exact source each installer came from, and you can always build it yourself with `npx tauri build`.

**Third-party code:** the backend depends only on [Tauri 2](https://v2.tauri.app), [`notify`](https://crates.io/crates/notify) (file watching), [`ureq`](https://crates.io/crates/ureq) with rustls (the one HTTPS request) and `serde`. The frontend has no runtime dependencies beyond the Tauri API and Tailwind at build time.

**Reporting a problem:** please open a [security advisory](https://github.com/lumilumiere/quotis/security/advisories/new) rather than a public issue.

## Build from source

You need [Node.js](https://nodejs.org) 18+, [Rust](https://rustup.rs), and the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for your OS (on Windows: the Visual Studio C++ Build Tools).

```bash
git clone https://github.com/lumilumiere/quotis.git
cd quotis
npm install
npx tauri dev      # run with hot reload
npx tauri build    # make an installer in src-tauri/target/release/bundle
```

Run the tests with `cargo test` inside `src-tauri/` (run `npm run build` once first). To check the Claude request against your own login: `cargo test live -- --ignored --nocapture`.

### How it's built

- **Backend**: Rust + [Tauri 2](https://v2.tauri.app), all in [`src-tauri/src/main.rs`](src-tauri/src/main.rs). A [`notify`](https://crates.io/crates/notify) file watcher follows the tools' log folders and pushes updates to the UI through Tauri events.
- **Frontend**: plain HTML/JS with Tailwind CSS: [`index.html`](index.html) + [`src/main.js`](src/main.js) for the widget, [`settings.html`](settings.html) + [`src/settings.js`](src/settings.js) for Settings.

## Contributing

Issues and pull requests are welcome, especially support for more tools, and testing on macOS and Linux. To add a tool, add a parser in `main.rs` next to the Codex/Gemini ones, a row in `src/main.js`, and a card in `src/settings.js`.

To publish a new release: bump `version` in `src-tauri/tauri.conf.json`, then push a tag like `v0.2.0`. GitHub Actions builds the installers for every OS and attaches them to a draft release.

## License

[MIT](LICENSE)
