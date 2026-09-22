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

Download the installer for your system from the [latest release](https://github.com/rjcfajardo/quotis/releases/latest):

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
- **Appearance**: theme (System, Dark or Light), glass opacity (how tinted the glass is; text always stays sharp), frosted blur, keep on top, and bars as % left or % used.
- **Refresh**: how often to check Claude's limits (1–60 min). Codex and Gemini update instantly when their logs change.

### Requirements per tool

- **Claude Code**: signed in with a Claude Pro or Max plan (API-key logins don't have 5-hour/weekly limits). If the row says *login expired*, open Claude Code once; it refreshes the login and Quotis picks it up.
- **Codex**: the Codex CLI signed in with ChatGPT. Limits appear after your first Codex message.
- **Gemini CLI**: any Gemini CLI install. Gemini doesn't store its quota locally, so Quotis shows token counts instead.

## Privacy

- Everything is read from files already on your computer. Nothing is uploaded or collected.
- The **only** network request is Claude's limit check: a small request to `api.anthropic.com`, sent with the login Claude Code already stored on your machine. Quotis never modifies or refreshes that login. On macOS, it reads it from the Keychain, which may ask you to allow access. Turn Claude off in Settings to stop the request entirely.
- The Claude endpoint is undocumented and may change. If it does, the Claude row shows an error instead of wrong numbers.

## Build from source

You need [Node.js](https://nodejs.org) 18+, [Rust](https://rustup.rs), and the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for your OS (on Windows: the Visual Studio C++ Build Tools).

```bash
git clone https://github.com/rjcfajardo/quotis.git
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
