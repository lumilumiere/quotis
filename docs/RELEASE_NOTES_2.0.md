# Quotis 2.0 release notes (draft)

Draft for the GitHub release. **2.0 has not been released.** Do not publish this until the items in [PROJECT_STATUS.md](PROJECT_STATUS.md) under "Blockers" are settled, and remove this paragraph when you do.

---

## Quotis 2.0

Quotis is now a small pill at the top of your screen instead of a sticky note. Click it and it opens into the full list of your AI coding tools and what each has left.

### What's new

- **Dynamic Island and Notch.** A floating pill you can drag anywhere, or a notch anchored to the top of the screen. The old always-open list is still there as Panel mode.
- **Honest numbers.** Every figure shows how fresh it is. If checks fail, the last numbers stay and say "stale". When a limit resets, Quotis shows "reset · updating…" until the provider confirms the new value, instead of guessing 100%.
- **A command line.** `quotis-cli status` prints the same numbers for scripts and coding agents. It only reads.
- **Two more providers,** both off until you turn them on: OpenRouter and DeepSeek, through their documented APIs.
- **More control.** Choose the screen, the theme, which provider the pill shows, when it warns you, whether it opens on hover, and whether it starts with your computer.

### What changed

- **Quotis no longer reads your screen.** The Windows "Real blur" option sampled the screen to draw a blur. It has been removed. Glass is now a translucent style; macOS still uses the system blur.
- The resizable widget is gone. Panel mode is the closest replacement.
- The default look is matte dark.
- Your 0.2 settings carry over.

### Fixed

Reset times shown hours off for accounts whose times carry a UTC offset; text from a provider being inserted into the page as markup; whole log files re-read on every change; several cases where a failure was shown as zero usage. The full list is in [CHANGELOG.md](../CHANGELOG.md).

### Before you install

- **Installers are not code-signed.** Windows SmartScreen and macOS Gatekeeper will warn you. Each download has a SHA-256 checksum on this page. See [INSTALL.md](INSTALL.md).
- **Tested on Windows 11.** The automated tests pass on macOS and Linux, but the new overlay has not been run on either. Reports from Mac and Linux users are very welcome.
- **OpenRouter and DeepSeek are experimental:** built from their documentation, not yet checked against a live account.
- The Claude Code, Codex, Copilot and Cursor numbers come from endpoints those vendors do not document. If one changes, Quotis shows an error for it rather than a wrong number.

### Downloads

| File | For |
|---|---|
| `Quotis_2.0.0_x64-setup.exe`, `Quotis_2.0.0_x64_en-US.msi` | Windows 10 / 11, x64 |
| `Quotis_2.0.0_aarch64.dmg` | macOS, Apple Silicon |
| `Quotis_2.0.0_x64.dmg` | macOS, Intel |
| `.AppImage`, `.deb`, `.rpm` | Linux |
| `quotis-cli-<target>` | The command-line tool |
| `SHA256SUMS-<target>.txt` | Checksums, with the commit each build came from |

Quotis is an independent project, not affiliated with or endorsed by Apple, Anthropic, OpenAI, Microsoft, GitHub, Google, Cursor, Alibaba, OpenRouter or DeepSeek.
