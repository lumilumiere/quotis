# Quotis 2.0 project status

Last updated 2026-11-16. Nothing is running in the background: no agents, no workers, no app.

## Milestone

Implementation complete and tested on Windows; CI green on four platforms. **Not released.** Blocked on the items under "Blockers".

## Where things are

| | |
|---|---|
| Branch | `quotis-2.0`, pushed to `origin`. `main` is untouched; no tag, no release |
| Final code commit | `fe4c1ef`. Later commits, if any, are documentation only |
| Baseline | `f80bc40` (`v0.2.1`) |
| Orca run | `run_21d09bc275ac`. Codex dispatches: baseline audit, implementation review, fix verification (one attempt lost to Codex's usage limit, then completed), re-check. No live workers |
| Commit author | Placeholder identity `Quotis Agent (Claude) <claude@quotis.invalid>`: no git identity is configured on this machine. These commits are already pushed |
| Commercial material | `commercial/`, git-ignored in this repo, its own local git repository, not pushed anywhere. Submission ZIP: `commercial/dist/quotis-ai-usage-intelligence-1.0.0.zip` |

## Latest test results

| Suite | Result | Where |
|---|---|---|
| Rust: core unit (67), engine (25), CLI contract (13) | 105 pass, 0 fail | Local Windows 11; CI on Windows, Ubuntu 22.04, macOS Apple Silicon, macOS Intel |
| Frontend view logic | 17 pass, 0 fail | Same |
| `cargo fmt --check`, `cargo clippy -D warnings` | Clean | Local and CI (clippy runs on Linux only) |
| `cargo audit` / `npm audit` | 0 vulnerabilities (2 upstream Rust warnings with no fix) | CI |
| Secret scan (gitleaks, full history) | Pass, with an allowlist of nine named fake test credentials | CI |
| CI on `fe4c1ef` | All 7 jobs green: https://github.com/lumilumiere/quotis/actions/runs/43809979978 | GitHub Actions |
| Desktop app, by hand through the WebView debugging port | Collapse / expand / Escape, Island / Notch / Panel, themes, glass, pinned provider, Settings window, hostile strings, overlapping saves: as expected | Windows 11 only, fixture data |
| Agent skill | 4 of 4 scenarios pass in Claude Code | Local |
| Windows installers | NSIS and MSI build from `fe4c1ef`. **Not installed or run** | Local |

## Independent reviews

All by Codex. None found a critical or high defect. Details and dispositions: [SECURITY_REVIEW.md](SECURITY_REVIEW.md).

| Review | Reviewed | Outcome |
|---|---|---|
| 0001 baseline | `f80bc40` | 18 findings |
| 0002 implementation | `f0b3ea5` | 16 new findings; baseline 7 fixed, 10 partial, 1 not fixed |
| 0003 fix verification | `cd8d662` | 6 verified, 9 partial, 1 accepted; 7 regressions. Release-blocking |
| 0004 re-check | `1ab7658` | Regressions largely verified fixed; 2 new medium; still release-blocking |
| Repairs after 0004 | `fe4c1ef` | **Not independently reviewed.** Codex's weekly limit was nearly spent |

Known open items that a reviewer flagged and were not fixed: HTTP-date `Retry-After`, no overall deadline for `quotis-cli --live`, collectors are not cancelled when they hang (only capped), a longer file replacement sharing its first 64 bytes looks like an append, walk errors inside a folder are skipped, the desktop shell has no automated tests, and no test drives the real DOM renderer.

## Blockers

Each needs the owner, or something this session did not have.

1. **An independent review of `fe4c1ef`.** Codex last judged the code release-blocking; the repairs since then are unreviewed. Re-run a Codex re-check against `fe4c1ef` when its weekly limit resets.
2. **No live-provider check.** No provider has been run against a real account in 2.0. Running the CLI against this machine's real logins was blocked by the session's permission system. To do it: `quotis-cli status --live`, compare with each tool's own usage screen, record in `PROVIDER_MATRIX.md`.
3. **macOS and Linux desktop behaviour.** CI proves the code builds and the tests pass there. Nobody has seen the overlay run on either.
4. **Installer smoke test.** Not run here because installing would replace any Quotis already installed on this machine.
5. **Code signing.** No certificates exist. Builds are unsigned.
6. **Agensi submission.** Needs a seller account and payout setup. Fees and payout terms were not read.
7. **Legal and naming.** Risk R1 (four providers use undocumented endpoints), R12 (no trademark search for "Quotis"), and the draft licence and privacy policy all need the owner's review.

## Things that happened that you should know

- **Two synthetic mouse clicks went to League of Legends.** While testing click-to-expand, two left clicks were sent at the top centre of the primary screen; the game was in the foreground and received them. Input injection was stopped and not used again.
- **The branch was pushed to the public repository** so CI could run on macOS and Linux.
- **Codex's update prompt was dismissed** with "skip until next version" several times; nothing was installed globally. Codex's weekly usage was mostly used by the reviews.
- **Rust toolchain components added:** `clippy`, `rustfmt`, and the `cargo-audit` tool.
- **Two actions were blocked by the permission system:** downloading and running gitleaks locally (it runs in CI instead), and a live check against real logins (blocker 2).

## Remaining risks

See [RISK_REGISTER.md](RISK_REGISTER.md). The ones that matter most: R1 (undocumented endpoints), R3 (unsigned), R4 (macOS untested on hardware).

## Next action

Blocker 1 (Codex re-check of `fe4c1ef`), then blocker 2 (live-provider check). The rest of [DEVELOPMENT_ROADMAP.md](DEVELOPMENT_ROADMAP.md) "To reach a 2.0 release" follows.
