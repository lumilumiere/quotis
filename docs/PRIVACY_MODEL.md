# Privacy model

Exactly what Quotis reads, keeps, sends and writes. Everything here can be checked in `crates/quotis-core/src/providers/`.

## Summary

- No telemetry, analytics, crash reporting or update check.
- The only network requests are usage checks to the provider whose login is used.
- Prompts and replies are never stored or sent.
- Quotis does not capture the screen. (Versions before 2.0 sampled it on Windows for a blur effect; that code is gone.)

## What is read

| Provider | Local data read | Kept in memory | Sent over the network |
|---|---|---|---|
| Claude Code | `.credentials.json` in the Claude folder; on macOS the Keychain item `Claude Code-credentials` | Usage percentages, reset times, plan name | Access token, to `api.anthropic.com` only |
| Codex | `auth.json`; the last 256 KB of the newest session logs | Limit percentages, reset times, plan name | Access token and account id, to `chatgpt.com` only |
| GitHub Copilot | `apps.json`, `hosts.json` (github.com entries only) | Premium request percentage, reset date, plan name | OAuth token, to `api.github.com` only |
| Cursor | One row of `state.vscdb` (the login token), opened read-only | Plan usage percentage, cycle end, plan name | Login token as a cookie, to `cursor.com` only |
| Gemini CLI, Qwen Code | New lines of `tmp/**/*.jsonl` | Timestamp and token count per message, for 24 hours | Nothing |
| opencode | Two aggregate queries on `opencode.db`, opened read-only | Token and cost totals | Nothing |
| OpenRouter (opt-in) | Environment variable `OPENROUTER_API_KEY` | Credit usage, key limit | The key, to `openrouter.ai` only |
| DeepSeek (opt-in) | Environment variable `DEEPSEEK_API_KEY` | Balance | The key, to `api.deepseek.com` only |

Disabled providers receive no new jobs or discovery. Work issued before disable can finish
or remain blocked at an OS boundary; changed-generation results are ignored.

## Prompts and replies

Bounded log text is parsed in memory to extract usage and detect unknown formats. Prompt
text on these lines can pass through memory briefly. Only numeric records and coverage
state are retained; prompt content is not persisted or transmitted by the collectors.

## What is written

| File | Location | Contents |
|---|---|---|
| `settings.json` | Per-user app config folder | Your Quotis settings. No credentials. |
| `settings.json.bak` | Same folder | Replica prepared during save. |
| `settings.committed.json` | Same folder | Authoritative completed settings generation. Prepared replicas cannot commit a failed save. |
| `snapshot.json` | Same folder | Normalized usage/status/reset/plan/error data. No credentials, paths or prompts. New Unix files use 0600; Windows inherits the directory ACLs. |

The folder is `%APPDATA%\io.github.lumilumiere.quotis` on Windows, `~/Library/Application Support/io.github.lumilumiere.quotis` on macOS, and `$XDG_CONFIG_HOME/io.github.lumilumiere.quotis` on Linux.

With "Start with the computer" on, Quotis also registers itself with the OS login items (a registry Run key on Windows, a LaunchAgent on macOS, an autostart entry on Linux).

Quotis never writes to another tool's files and never refreshes or changes a login.

## Network

- At most one request per online provider per refresh interval (default 5 minutes), plus an earlier re-check no sooner than 2 minutes after that tool's files change or a quota window resets, plus manual refreshes (30-second floor). Copilot may try each stored github.com login once per check.
- Each request is a TLS GET with a 10-second timeout and no redirects.
- Four of the endpoints are undocumented (Claude Code, Codex, Copilot, Cursor). They are the endpoints those tools use themselves. Vendors may change or restrict them.

## The CLI and the agent skill

`quotis-cli` reads `snapshot.json` and prints it. `export` removes plan names. `--live` performs the same reads and requests as the desktop app, once. The CLI writes nothing.

The Quotis agent skill is a set of text instructions. It asks the agent to run `quotis-cli` or to read an export you paste. It contains no code that runs, makes no network requests of its own, and has no access to logins. What an AI agent does with usage numbers you give it is governed by that agent's own privacy terms.

## Your controls

- Turn any provider off in Settings to stop all reading and requests for it.
- Delete the config folder to remove everything Quotis wrote.
- Quit Quotis to stop all activity; it has no background service.
