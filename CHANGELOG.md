# Changelog

## 2.0.0 (unreleased)

Developed and tested on Windows 11. The test suite also passes on macOS (Apple Silicon and Intel) and Linux in CI, but the overlay itself has not yet been run on a Mac or on Linux.

### New

- **Dynamic Island mode:** a floating pill showing your tightest limit, with a dot per other provider. Click to open the full list. Draggable; remembers where you put it.
- **Notch mode:** the same pill anchored to the top centre of the screen, opening downward.
- **Panel mode:** the list, always open.
- **Screen choice** for multi-monitor setups.
- **Light and dark themes**, or follow the system. Reduced-motion settings are respected.
- **Expand on hover**, **auto-collapse**, a **warning threshold**, a choice of which provider the pill shows, and **start with the computer**.
- **Manual refresh** button.
- **`quotis-cli`:** read-only command-line access to the same data, with versioned JSON output and defined exit codes.
- **OpenRouter** and **DeepSeek** providers, using their documented APIs. Off by default; each needs an API key in an environment variable. Built from documentation and not yet verified against a live account.
- Claude Code: per-model weekly windows are shown when Anthropic's response includes them.
- Keyboard access: Enter or Space opens the pill, Esc closes it, every control is reachable by Tab.

### Changed

- **Numbers now carry their age.** When checks fail, the last numbers stay and are marked "stale" with the reason, instead of looking current forever.
- **A passed reset no longer shows 100% left.** The row shows "reset · updating…" until the provider confirms the new value, and Quotis re-checks within about two minutes.
- Codex window labels come from the data. If the data has no window length, the row says "primary" / "secondary" instead of assuming 5-hour and weekly.
- A Copilot row is labelled "premium" rather than "mo".
- Settings lists providers from the app itself and states, for each, which host it contacts and whether that endpoint is documented.
- A provider that is turned off is no longer read at all, including its stored login.
- The default look is matte dark. Glass is an option.
- The app's UI permissions were reduced to events, window drag and window close.

### Removed

- **The Windows "Real blur" screen sampling.** Quotis no longer reads the screen in any way. Glass on Windows is now a translucent style without background blur; macOS still uses the system blur.
- The resizable sticky-note widget and its scale-to-fit behaviour. Panel mode is the closest replacement.

### Fixed

- Timestamps with a UTC offset (such as `+08:00`) were read as UTC, shifting reset times by the offset. Invalid dates such as February 30 were accepted.
- Text from a provider (a Cursor plan name) was inserted into the page as HTML.
- The Windows blur read strips of the screen outside the widget, about 20 times a second.
- Whole log files were re-read on every change.
- An empty response from Anthropic was treated as success and shown as zero usage.
- A failed opencode database query was shown as zero usage.
- Log entries dated in the future counted as recent usage.
- Folder scans followed symlinks and Windows junctions.
- A slow check could overwrite a newer result, or land after its folder setting had changed.
- Copilot logins for other hosts (GitHub Enterprise) were sent to `api.github.com`.
- `settings.json` could be left half-written.
- A database query ran while the app's state lock was held, so one slow disk could stall the UI.

### Security

- Two high-severity advisories in the Vite development server (a build-time dependency) were resolved by updating it.
- Seven advisories in locked Rust dependencies carried over from 0.2 (the rustls TLS stack and the bundled SQLite) were resolved by updating them.
- Freshness is re-judged whenever a snapshot is read, so a number never looks current only because it was current when written.
- A damaged settings file falls back to the last good copy instead of turning every provider back on.

### For developers

- The single-file backend is now a workspace: `quotis-core` (engine and provider adapters), `quotis-cli`, and the Tauri shell. Adding a provider is one file.
- 99 Rust tests and 17 frontend tests, none of which use the network or real logins.
- New CI workflow for pushes and pull requests: formatting, clippy, tests on Windows, macOS (Apple Silicon and Intel) and Linux, dependency advisories, secret scanning.
- Releases attach `quotis-cli` binaries and SHA-256 checksum files, and support macOS signing and notarization when the certificates are provided.

## 0.2.1

Check Codex limits live from OpenAI.

## 0.2.0

Add GitHub Copilot, Cursor, Qwen Code and opencode.
