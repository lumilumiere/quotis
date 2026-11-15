# Threat model

What Quotis protects, from whom, and where the limits are.

## Assets

| Asset | Where it lives | Why it matters |
|---|---|---|
| Provider logins (OAuth tokens, API keys) | Other tools' files, the macOS Keychain, environment variables | Anyone holding one can use the account |
| Prompts and replies | Other tools' logs and databases | Private work content |
| Usage numbers, plan names | Quotis memory, `snapshot.json` | Low sensitivity; reveals which tools a person uses |
| Settings | `settings.json` | Controls which folders are read and which hosts are contacted |
| The user's trust in the numbers | The UI and CLI output | A wrong "62% left" causes real planning mistakes |

## Trust boundaries

1. **Provider servers → Quotis.** Responses are untrusted input: parsed defensively, strings cleaned, size-capped.
2. **Local tool files → Quotis.** Logs and databases can be malformed, huge, truncated, or planted by another local program.
3. **Webview → Rust.** The page can call nine commands and nothing else.
4. **Other local processes → `snapshot.json` / `settings.json`.** Same-user processes can read and write these. Quotis does not defend against malware running as the user; nothing does.
5. **Agent → `quotis-cli`.** A coding agent may be steered by prompt injection. The CLI must be harmless whatever arguments it is given.
6. **Build pipeline → users.** Installers come from GitHub Actions.

## Threats and mitigations

| # | Threat | Mitigation | Residual risk |
|---|---|---|---|
| T1 | A token is sent to the wrong host | Each adapter has one fixed HTTPS URL. Redirects are not followed. Copilot uses only `github.com` logins. | A compromised vendor domain. |
| T2 | A token leaks into logs, the snapshot, or CLI output | Nothing logs headers. `ProviderError` carries fixed messages. Snapshot types have no field for secrets. Tests assert fake tokens never appear in output. | Tokens are in process memory while a request runs. |
| T3 | Provider or log text is rendered as HTML | All dynamic text goes through `textContent`. CSP `default-src 'self'` blocks scripts as a second layer. Strings are stripped of control characters and capped at 120 characters. | None known. |
| T4 | Prompt content is collected | Collectors keep only numbers and timestamps. Lines without the marker field are skipped before JSON parsing. | A matching log line is parsed whole in memory, then dropped. |
| T5 | A planted symlink or junction leads a scan elsewhere | Walks never follow links found inside a provider folder, are depth-limited to 8, and cap files and entries per folder. | The configured folder itself may be a link (people keep tool folders on other drives); what it points to is read. |
| T6 | A huge or hostile file exhausts memory | Reads are capped (16 MB per pass, 256 KB tails, 1 MB credentials, 1 MB HTTP bodies, 4 MB snapshot). Entries older than 24 h are dropped. | A log with one line over 16 MB is skipped. |
| T7 | The webview is tricked into reading arbitrary files | No file-system, shell or HTTP capability. The only path input is a provider folder; a relative or `~user` path is refused, and only fixed file names inside it are read. | A user can point a provider at any folder they can already read. |
| T8 | An agent is steered into misusing the CLI | Five fixed read commands. No file-path, URL or command argument. An unknown provider id is rejected and not echoed. | `--live` makes the same network calls the desktop app makes. |
| T9 | Another local user reads the snapshot | File mode 0600 on Unix; per-user profile ACL on Windows. | Same-user malware. |
| T10 | Wrong numbers presented as true | Freshness on every metric; expired windows never show a value as current; categories never convert into each other; failed queries are errors, not zeros. | Undocumented endpoints can change meaning without changing shape. |
| T11 | A provider endpoint changes | A response without the expected fields is `bad_response`, shown as an error. | See T10. |
| T12 | A provider is hammered | One poll in flight, backoff to one hour, `Retry-After` honoured, 30-second floor on manual refresh. | None known. |
| T13 | Compromised dependency | `Cargo.lock` and `package-lock.json` committed; CI runs `cargo audit`, `npm audit` and builds with `--locked`. | A malicious release of a direct dependency before an advisory exists. |
| T14 | Tampered installer | Built by GitHub Actions from the tagged commit; SHA-256 sums attached. | **Installers are unsigned** until signing certificates are configured. Users see OS warnings and cannot verify the publisher. |
| T15 | Malicious auto-update | There is no updater and no update check. | Users must update by hand, so fixes reach them slowly. |
| T16 | Agent skill oversteps | The skill is text only: no scripts, no network, reads only `quotis-cli` output or a pasted export. | The buyer's agent decides what to run; the skill cannot enforce it. |

## Out of scope

- Malware or another person with access to the user's OS account.
- Physical access to an unlocked machine.
- The security of the monitored tools' own credential storage.
- Vendor-side account compromise.

## Open risks carried into 2.0

- Four adapters depend on undocumented endpoints and stored logins the vendors did not offer for third-party use. Vendors could object or block. See [RISK_REGISTER.md](RISK_REGISTER.md) R1.
- Unsigned installers (T14).
- macOS Keychain access is by shelling out to `/usr/bin/security`; behaviour on current macOS is unverified on hardware.
