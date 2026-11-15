# Quotis

A small always-on-top pill that shows how much of your AI coding limits you have left: **Claude Code**, **Codex**, **GitHub Copilot**, **Cursor**, **Gemini CLI**, **Qwen Code**, **opencode**, and optionally **OpenRouter** and **DeepSeek**.

It sits at the top of your screen as a floating island or a notch. Click it and it opens into the full list. A read-only command-line tool, `quotis-cli`, gives the same numbers to scripts and coding agents.

> Quotis is an independent project. It is not affiliated with or endorsed by Apple, Anthropic, OpenAI, Microsoft, GitHub, Google, Cursor, Alibaba, OpenRouter or DeepSeek.

**Status of 2.0:** developed and tested on Windows 11. macOS and Linux builds are produced by CI but the 2.0 overlay has **not yet been tested on a Mac or on Linux**. See [docs/PROJECT_STATUS.md](docs/PROJECT_STATUS.md).

## What it shows

| Tool | What Quotis shows | Where it comes from |
|---|---|---|
| **Claude Code** | 5-hour and weekly limits: % left and time until reset | Anthropic's usage endpoint (the one Claude Code's `/usage` uses), with your existing Claude Code login |
| **Codex** | Its two rate-limit windows: % left and time until reset | OpenAI's usage endpoint (the one Codex's `/status` uses) with your existing Codex login, plus Codex's local session logs for instant updates |
| **GitHub Copilot** | Monthly premium requests: % left and reset date | GitHub's Copilot quota endpoint, with the login your editor plugin stored |
| **Cursor** | Included plan usage this billing cycle | Cursor's usage endpoint, with the login the Cursor app stored |
| **Gemini CLI** | Tokens used in the last 5 and 24 hours | Gemini CLI's local logs |
| **Qwen Code** | Tokens used in the last 5 and 24 hours | Qwen Code's local logs |
| **opencode** | Tokens and estimated cost in the last 5 and 24 hours | opencode's local database |
| **OpenRouter** (off by default) | Credits used, key limit if set | OpenRouter's documented key API, with `OPENROUTER_API_KEY` |
| **DeepSeek** (off by default) | Prepaid balance | DeepSeek's documented balance API, with `DEEPSEEK_API_KEY` |

Quotis keeps three kinds of number apart: a **limit the provider reported**, **usage it measured locally**, and a **cost estimate**. A token count is never turned into a "% left", and no limit is invented for a tool that does not report one. The full list, with what has and has not been verified, is in [docs/PROVIDER_MATRIX.md](docs/PROVIDER_MATRIX.md).

Every number carries its age. If checks fail, the last numbers stay and are marked **stale**. When a limit's reset time passes, Quotis shows **"reset · updating…"** until the provider confirms the new value, instead of assuming you are back to 100%.

## Install

See [docs/INSTALL.md](docs/INSTALL.md). In short: download the installer for your system from the [latest release](https://github.com/lumilumiere/quotis/releases/latest). **Builds are not code-signed**, so Windows SmartScreen and macOS Gatekeeper warn on first launch.

## Using it

- **Open / close:** click the pill, or focus it and press Enter. Esc closes.
- **Move:** drag the island. The notch stays anchored to the top centre.
- **Refresh, Settings, Quit:** the three buttons at the top right of the open panel.

### Settings

- **Display:** Dynamic Island, Notch or Panel (always open); which screen; which provider the pill shows (automatic picks the tightest limit); expand on hover; auto-collapse; the warning threshold.
- **Appearance:** dark, light or match the system; matte or translucent glass; % left or % used; keep on top; start with the computer. Reduced-motion system settings are respected.
- **Connections:** turn each tool on or off and, if it lives somewhere unusual, set its folder. Each entry says which host it contacts and whether that endpoint is documented.
- **Refresh:** how often to check online limits (1 to 60 minutes).

## Command line

```
quotis-cli status                      # one line per connected provider
quotis-cli snapshot                    # full JSON snapshot
quotis-cli providers                   # every known provider and what it contacts
quotis-cli diagnose --provider codex   # why a provider is or is not reporting
quotis-cli export                      # sanitized JSON to share or analyse elsewhere
```

By default the CLI reads the snapshot file the desktop app keeps current and makes no network request. `--live` collects once itself; `--live --offline` reads local files only. It never writes anything. Exit codes: 0 ok, 2 bad usage, 3 no snapshot, 4 snapshot stale (still printed), 5 incompatible version, 6 malformed snapshot, 7 unknown provider. JSON output is versioned (`schema_version: 1`).

## Privacy and security

- No telemetry, analytics, crash reporting or update check.
- The only network requests are usage checks, each sent only to the provider whose login is used. Turn a provider off and nothing of it is read or contacted.
- Quotis reads the logins your tools already stored and never writes, refreshes, copies or logs them. On macOS, Claude Code's login is in the Keychain, which asks your permission.
- Prompts and replies are never stored or sent.
- **Quotis does not capture your screen.** Versions before 2.0 sampled the screen on Windows to draw a blur; that code has been removed.
- It writes two files in your user config folder: `settings.json` and `snapshot.json` (usage numbers only, readable by your account only).
- The UI can listen for events, drag its window and close itself. It has no shell, file-system or HTTP access, loads only files bundled in the app, and inserts all dynamic text as plain text.
- The Claude Code, Codex, Copilot and Cursor endpoints are undocumented and may change. If one does, that provider shows an error instead of a wrong number.

Details: [docs/PRIVACY_MODEL.md](docs/PRIVACY_MODEL.md), [docs/THREAT_MODEL.md](docs/THREAT_MODEL.md), [docs/SECURITY_REVIEW.md](docs/SECURITY_REVIEW.md). To report a vulnerability, open a [security advisory](https://github.com/lumilumiere/quotis/security/advisories/new) rather than a public issue.

## Upgrading from 0.2

- Your settings carry over.
- The resizable sticky-note widget is replaced by the island, notch and panel modes.
- "Real blur" is gone on Windows (see above). Glass is now a translucent style; macOS still uses the system blur.
- The default look is matte dark. Turn on Glass in Settings for the old feel.

## Build from source

You need [Node.js](https://nodejs.org) 20+, [Rust](https://rustup.rs), and the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for your OS.

```bash
git clone https://github.com/lumilumiere/quotis.git
cd quotis
npm install
npx tauri dev                       # run with hot reload
npx tauri build                     # installer in target/release/bundle
cargo build --release -p quotis-cli # the CLI
```

Tests: `npm test` and `cargo test --workspace` (run `npm run build` once first). Neither uses the network or your real logins.

### How it's built

- **`crates/quotis-core`**: the monitoring engine. Provider adapters, the normalized usage model, scheduling, the snapshot file.
- **`crates/quotis-cli`**: the CLI.
- **`src-tauri`**: the desktop shell (Rust + [Tauri 2](https://v2.tauri.app)).
- **`src/`**: the overlay and Settings pages, plain HTML/JS/CSS.

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Contributing

Issues and pull requests are welcome, especially testing on macOS and Linux and support for more tools. Adding a tool is one Rust file: see [docs/PROVIDER_INTEGRATION_GUIDE.md](docs/PROVIDER_INTEGRATION_GUIDE.md).

To publish a release: set the version in `Cargo.toml`, `src-tauri/tauri.conf.json` and `package.json`, update `CHANGELOG.md`, then push a tag like `v2.0.0`. GitHub Actions builds installers and the CLI for every OS and attaches them, with checksums, to a draft release.

## License

[MIT](LICENSE)
