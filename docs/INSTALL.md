# Installing Quotis

## Requirements

| | Minimum | Tested with 2.0 |
|---|---|---|
| Windows | Windows 10 or 11, x64, with the WebView2 runtime (preinstalled on Windows 11) | Windows 11 Pro, WebView2 157 |
| macOS | macOS 10.15 or later, Apple Silicon or Intel | **Not yet tested** |
| Linux | A desktop with WebKitGTK 4.1 | **Not yet tested** |

"Not yet tested" means the 2.0 overlay has not been run on that system. The builds exist; their behaviour is unconfirmed.

## Signing status

**Installers are not code-signed.** No signing certificate has been configured for this project. Each release is built by GitHub Actions from the tagged commit, and every release carries `SHA256SUMS-<target>.txt` files so you can check a download.

To check a download:

```
# Windows (PowerShell)
Get-FileHash .\Quotis_2.0.0_x64-setup.exe -Algorithm SHA256

# macOS / Linux
shasum -a 256 Quotis_2.0.0_aarch64.dmg
```

Compare the result with the line for that file in the matching `SHA256SUMS` file.

## Windows

1. Download `Quotis_<version>_x64-setup.exe` (or the `.msi`) from the [latest release](https://github.com/lumilumiere/quotis/releases/latest).
2. Run it. SmartScreen shows "Windows protected your PC" because the installer is unsigned: choose **More info**, then **Run anyway**.
3. Quotis appears as a small pill at the top centre of your primary screen.

Uninstall from **Settings > Apps**. To remove its data too, delete `%APPDATA%\io.github.lumilumiere.quotis`.

## macOS

1. Download the `.dmg`: `aarch64` for Apple Silicon, `x64` for Intel.
2. Drag Quotis to Applications.
3. Because the app is unsigned and not notarized, macOS may say it is damaged or from an unidentified developer. Run this once, then open it again:
   ```
   xattr -cr /Applications/Quotis.app
   ```
4. The first time Quotis checks Claude Code, macOS asks whether it may read the "Claude Code-credentials" Keychain item. Deny it if you prefer, and turn Claude Code off in Settings.

Known limits on macOS: the notch sits just below the menu bar rather than inside it, including on MacBooks with a camera notch. Third-party apps cannot draw in the camera housing area.

Data folder: `~/Library/Application Support/io.github.lumilumiere.quotis`.

## Linux

Download the `.AppImage`, `.deb` or `.rpm`. Transparent, always-on-top windows depend on your compositor; on some Wayland setups the overlay cannot position itself or stay on top.

Data folder: `$XDG_CONFIG_HOME/io.github.lumilumiere.quotis` (usually `~/.config/…`).

## The command-line tool

`quotis-cli` is a separate download on the release page: `quotis-cli-<target>` (add `.exe` on Windows). Put it in a folder on your `PATH`.

```
quotis-cli --version
quotis-cli status
```

`quotis-cli` and the desktop app must come from the same release.

## First run

Quotis looks for each tool in its usual place (`~/.claude`, `~/.codex`, and so on, respecting `CLAUDE_CONFIG_DIR` and `CODEX_HOME`). Tools it finds appear by themselves. Open the pill and choose the Settings button to turn tools off, point one at a different folder, or switch between Island, Notch and Panel.

### OpenRouter and DeepSeek

These are off by default and need an API key in an environment variable:

- `OPENROUTER_API_KEY`
- `DEEPSEEK_API_KEY`

Set the variable for your whole user session (Windows: **Edit environment variables for your account**; macOS: `launchctl setenv`; Linux: your session environment), restart Quotis, then turn the provider on in Settings. A variable set only in a terminal profile is not visible to an app started from the desktop. Quotis reads the key when it makes the request and never stores it.

## Troubleshooting

| Problem | Try |
|---|---|
| A tool does not appear | Settings > Connections shows where Quotis looked and whether it found the tool. `quotis-cli diagnose --provider <id>` explains further |
| "login expired" | Open that tool once so it refreshes its own login |
| Numbers say "stale" | Checks are failing or the computer was asleep. Open the pill for the reason; it refreshes on the next successful check |
| "reset · updating…" | The limit window just reset; Quotis re-checks within about two minutes |
| The pill is on the wrong screen | Settings > Display > Screen |
| The pill covers something | Drag it (Island mode), or turn off **Keep on top** |
| `quotis-cli` exits with 3 | The desktop app has not run; start it or add `--live` |
