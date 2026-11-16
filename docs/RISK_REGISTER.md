# Risk register

Likelihood and impact are High / Medium / Low. Status is as of the 2.0 development branch.

| ID | Risk | Likelihood | Impact | Mitigation | Status |
|---|---|---|---|---|---|
| R1 | Vendor restricts undocumented endpoints/stored logins (Claude Code, Codex, Copilot, Cursor) | Medium | High: integration or terms problems | Registry/Settings/matrix flag undocumented endpoints. Shape errors surface; semantic drift may still give wrong numbers. No stability guarantee in FREE copy | Open; owner terms/legal judgement required before public claims |
| R2 | An undocumented response changes meaning without changing shape, so a wrong number is shown as fresh | Low | High | Minimum-shape validation; fixtures per provider; window lengths come from the response where the response gives them | Accepted |
| R3 | Installers are unsigned: OS warnings, no publisher verification | Certain | Medium | Checksums and traceable CI builds; signing wired into the release workflow, waiting for certificates | Open, blocked on certificates |
| R4 | macOS behaviour is untested on hardware: Keychain prompt, window level, Spaces, vibrancy, notarization | High | High for a macOS release | CI runs the test suite on macOS runners; that covers logic, not window behaviour | **Open. macOS must not be advertised as supported until tested on a Mac.** |
| R5 | OpenRouter and DeepSeek adapters were built from documentation and never run against a live account | Medium | Low: opt-in providers | Fixtures mirror the documented shapes; marked "unverified live" in the provider matrix | Open |
| R6 | The overlay covers a borderless-fullscreen game or presentation | Medium | Low | "Keep on top" switch; fullscreen detection deferred (ADR-11) | Accepted for 2.0 |
| R7 | Providers change telemetry shape or meaning | Medium | Medium | Unknown token schemas, malformed lines and listing errors qualify lower bounds; CLI/overlay disclose them. Valid-looking semantic changes can escape validation | Open for live comparison; ordinary malformed-zero path repaired |
| R8 | WebView2 memory is outside Quotis's control and may exceed the 120 MB target | Medium | Low | Measured in the performance report; no frameworks, tiny page | See PERFORMANCE_REPORT.md |
| R9 | A GUI-launched app does not see environment variables set only in a shell profile, so API-key providers look "not found" | High on macOS | Low | Documented in the install guide | Accepted |
| R10 | Agensi rejects the FREE skill or changes rules | Medium | Medium | Ten text files, packaging/pricing checked 2026-11-16; no installer. FREE needs no payout setup | Not submitted; delivery UNVERIFIED |
| R11 | Optional local workflows require unreleased 2.0 CLI | High | Medium | FREE listing discloses dependency/live limitations; supplied exports or historical synthetic sample work offline | Offline structure/arithmetic pass; marketplace delivery UNVERIFIED |
| R12 | "Quotis" conflicts with an existing trademark | Unknown | High | Not researched | **Open. Needs a trademark search before commercial launch.** |
| R13 | Tokens-per-window for Gemini/Qwen is read as a quota by users | Low | Medium | Shown as plain numbers with no bar; category `local_usage` in the data | Mitigated |
| R14 | Clock changes on the user's machine distort freshness | Low | Low | Observations from the future are marked stale; future-dated log records are not counted | Mitigated |
| R15 | OS filesystem calls hang past caller deadline | Medium | Medium | Three local/three online jobs per provider across generations; explicit cap errors and duplicate accounting; 60s live collector wait | Scoped accepted limit; no arbitrary thread cancellation. Restart may be needed |
| R16 | Commit record corrupt or downgrade ignores it | Low | Medium | Fail closed with providers disabled. Save reviewed settings to recover; migrate authoritative record deliberately | Recovery tested; downgrade/power-loss behavior UNVERIFIED |
| R17 | Native Windows UI/installer checks incomplete | Medium | Medium | Scoped runtime evidence preserved; no global input. WebView debugger unreachable; no clean VM | UNVERIFIED blocks public release |
| R18 | Linux GTK/build dependency warnings | Medium | Medium | glib unsound iterator/proc-macro-error warnings retained; Tauri dependency line constrains upgrades | Open upstream; scanners do not prove safe native behavior |
| R19 | Limited pill tooltip/ranking assurance | Low | Low | Fresh reviewer: partial local count now visible; exact cached-local error remains in expanded card, not pill tooltip. Mixed complete/partial quota windows may rank complete subset, but no current adapter emits partial quota | Accepted nonblocking; add coverage before introducing partial quota adapters |
| R20 | Native placement API rejects a requested move | Low | Low | Destination marker is set before unchecked native placement; if OS rejects it without an event, a later move to exactly that point can be ignored | Source-only, nonblocking; native failure-path certification UNVERIFIED |

## Review cadence

Re-check R1, R2 and R7 whenever a monitored tool ships a major version. Re-check R10 before each marketplace submission.
