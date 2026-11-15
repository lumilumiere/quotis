# Baseline audit: Quotis v0.2.1

What the project looked like before 2.0, checked against the source rather than assumed.
An independent second audit by Codex is in [reviews/0001-baseline-codex.md](reviews/0001-baseline-codex.md).

## Baseline

| Item | Value |
|---|---|
| Commit | `f80bc4062ca589a79ef5dbb7a4f45b434b3aa3f7` (tag `v0.2.1`), clean working tree |
| Source size | `main.rs` 1,127 lines, `main.js` 215, `settings.js` 88, `style.css` 211 |
| Rust dependencies | tauri 2.11.6 (`macos-private-api`), notify 8, ureq 3.4.2, rusqlite 0.40.2 (bundled), serde, windows-sys 0.59 |
| npm dependencies | @tauri-apps/api 2.11, @tauri-apps/cli 2.11, vite 8.3, tailwindcss 4.3 |
| Tests | 9 Rust tests in `main.rs`: 8 pass, 1 ignored (live network). No frontend tests. |
| CI | One workflow, on version tags only: build, test, draft release for Windows, macOS (arm64, x64), Linux |
| Toolchain used | Windows 11 Pro 10.0.26300, rustc 1.98.1, Node 24.15.0 |

Reproduced: `npm ci`, `npm run build` (Vite, 323 ms), `cargo test` (8 passed, 1 ignored; first compile 8 min 03 s).
`npm audit` at baseline reported 2 high-severity advisories in Vite's dev server (dev-time only).

Not measured at baseline: startup time, CPU and memory. The 0.2.1 app was not profiled before the rewrite, so 2.0's performance report has no before-number.

## Runtime permissions and data access at baseline

- Tauri capability: `core:default` plus window drag, resize-drag and close, for both windows.
- Five commands: `get_usage`, `get_settings`, `get_status`, `save_settings`, `open_settings`.
- Reads: Claude Code credentials file (macOS: Keychain via `security`), Codex `auth.json`, Copilot `apps.json` / `hosts.json`, Cursor `state.vscdb`, Codex / Gemini / Qwen JSONL logs, opencode SQLite DB.
- Writes: `settings.json` only.
- Network: four HTTPS GETs (Anthropic, ChatGPT backend, GitHub, Cursor), all to undocumented endpoints.
- Windows only: GDI screen sampling for the blur.

## The ten stated concerns

| # | Concern | Verdict | Evidence at `f80bc40` |
|---|---|---|---|
| A | Backend concentrated in `main.rs` | Confirmed | One 1,127-line file holds settings, credentials, HTTP, parsers, watcher, GDI, commands and tests. |
| B | Provider interfaces hard-coded | Confirmed | A provider is a named field in `Settings`, `Dirs`, `Snapshot`, a `match` arm in `row()`, `enabled()`, `poll()`, and a row in two JS arrays. `Row` has fixed `five_hour` / `weekly` / `monthly` fields. |
| C | Undocumented endpoints | Confirmed | `api.anthropic.com/api/oauth/usage`, `chatgpt.com/backend-api/wham/usage`, `api.github.com/copilot_internal/user`, `cursor.com/api/usage-summary`. The README says so itself. |
| D | Whole log files re-read | Confirmed | `codex_row` reads up to 20 complete session files on every file event; `token_entries` re-reads the whole file on every change. |
| E | Timezone offsets mishandled | Confirmed | `parse_ts` reads fixed digit positions and ignores everything after the seconds. `…T00:00:00+08:00` and `…T00:00:00Z` give the same instant. It also accepts February 30. |
| F | Expired windows show unverified quota | Confirmed | `left()` in `main.js` returns 100 as soon as `resets_at` passes, with no refresh. |
| G | Unsafe HTML rendering | Confirmed, limited by CSP | Cursor's `membershipType` flows into `Row.status` and into `innerHTML`. The CSP blocks scripts, so the reachable impact is markup injection, not code execution. The README's "no user or file content is ever rendered as HTML" was wrong. |
| H | Blur by repeated screen sampling | Confirmed | A thread wakes every 50 ms and samples four strips **outside** the window before any throttle, then grabs the patch behind the window up to five times a second. |
| I | Weak error and offline handling | Confirmed | After one success, `Unavailable` and 429 leave the old numbers on screen forever with no stale mark. Codex errors are dropped entirely. Every lock is `.lock().unwrap()`. |
| J | Insufficient tests | Confirmed | Eight tests, happy paths only. Nothing on offsets, stale data, concurrency, incremental reads, injection. |

## Further defects found

From this audit and Codex's:

- The state mutex is held across SQLite queries and log scans (`emit`, `apply`).
- Empty Claude response `{}` parses as a successful, limit-less row.
- opencode query failures are reported as zero usage.
- Future-dated records count as recent usage.
- Directory walks follow symlinks and Windows junctions, with no depth limit.
- A slow poll can overwrite a newer result; a result for an old folder can land after the folder changed.
- `settings.json` is written non-atomically.
- Copilot tokens for any host are sent to `api.github.com`.
- Disabled providers still have their credentials read by `detect`.

## Disposition

All ten concerns and the further defects are addressed in 2.0. Each fix and its regression test is listed in [SECURITY_REVIEW.md](SECURITY_REVIEW.md); Codex's independent verification is in [reviews/](reviews/).
