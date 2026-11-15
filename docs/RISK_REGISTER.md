# Risk register

Likelihood and impact are High / Medium / Low. Status is as of the 2.0 development branch.

| ID | Risk | Likelihood | Impact | Mitigation | Status |
|---|---|---|---|---|---|
| R1 | A vendor blocks or objects to use of its undocumented usage endpoint or stored login (Claude Code, Codex, Copilot, Cursor). This is also a terms-of-service question, not only a technical one. | Medium | High: the headline providers stop working; possible takedown request | Endpoints are flagged `documented: false` in the registry, Settings and the provider matrix. Failures show as errors, never wrong numbers. Commercial copy must not promise these integrations will keep working. | **Open. Needs the owner's legal judgement before any paid listing leans on these providers.** |
| R2 | An undocumented response changes meaning without changing shape, so a wrong number is shown as fresh | Low | High | Minimum-shape validation; fixtures per provider; window lengths come from the response where the response gives them | Accepted |
| R3 | Installers are unsigned: OS warnings, no publisher verification | Certain | Medium | Checksums and traceable CI builds; signing wired into the release workflow, waiting for certificates | Open, blocked on certificates |
| R4 | macOS behaviour is untested on hardware: Keychain prompt, window level, Spaces, vibrancy, notarization | High | High for a macOS release | CI runs the test suite on macOS runners; that covers logic, not window behaviour | **Open. macOS must not be advertised as supported until tested on a Mac.** |
| R5 | OpenRouter and DeepSeek adapters were built from documentation and never run against a live account | Medium | Low: opt-in providers | Fixtures mirror the documented shapes; marked "unverified live" in the provider matrix | Open |
| R6 | The overlay covers a borderless-fullscreen game or presentation | Medium | Low | "Keep on top" switch; fullscreen detection deferred (ADR-11) | Accepted for 2.0 |
| R7 | Gemini / Qwen / Codex / opencode change their local log format | Medium | Medium | Parsers ignore lines they do not understand; a format change shows as zero usage for token logs (cannot be told apart from no activity) | Accepted; noted in the matrix |
| R8 | WebView2 memory is outside Quotis's control and may exceed the 120 MB target | Medium | Low | Measured in the performance report; no frameworks, tiny page | See PERFORMANCE_REPORT.md |
| R9 | A GUI-launched app does not see environment variables set only in a shell profile, so API-key providers look "not found" | High on macOS | Low | Documented in the install guide | Accepted |
| R10 | Agensi rejects the skill, or its rules change | Medium | Medium for the commercial plan | Skill is text-only and follows the published packaging and security rules as read on 2026-11-15 | Open until submitted |
| R11 | The paid skill's value depends on the free CLI being installed | Medium | Medium | Offline mode works on a pasted export with no install | Mitigated by design; untested with real buyers |
| R12 | "Quotis" conflicts with an existing trademark | Unknown | High | Not researched | **Open. Needs a trademark search before commercial launch.** |
| R13 | Tokens-per-window for Gemini/Qwen is read as a quota by users | Low | Medium | Shown as plain numbers with no bar; category `local_usage` in the data | Mitigated |
| R14 | Clock changes on the user's machine distort freshness | Low | Low | Observations from the future are marked stale; future-dated log records are not counted | Mitigated |

## Review cadence

Re-check R1, R2 and R7 whenever a monitored tool ships a major version. Re-check R10 before each marketplace submission.
