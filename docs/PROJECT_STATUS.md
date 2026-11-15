# Quotis 2.0 project status

Last updated 2026-11-16. Nothing is running in the background: no agents, no workers, no app.

## Milestone

Implementation complete on Windows and green in CI on four platforms. **Not released.** Blocked on the items under "Blockers".

## Where things are

| | |
|---|---|
| Branch | `quotis-2.0`, pushed to `origin` (a development branch; `main` is untouched, no tag, no release) |
| Integrated commit | see `git log -1` on the branch; code is final at `cd8d662`, later commits are documentation only |
| Baseline | `f80bc40` (`v0.2.1`) |
| Orca run | `run_21d09bc275ac`. Four Codex dispatches: two completed, one failed at start (update prompt, retried), one stopped (Codex usage limit). No live workers |
| Commit author | Placeholder identity `Quotis Agent (Claude) <claude@quotis.invalid>`: no git identity is configured on this machine. These commits are already pushed |
| Commercial material | `commercial/`, git-ignored here, its own local git repository, not pushed anywhere |

## Latest test results

| Suite | Result | Where |
|---|---|---|
| Rust: core unit (63), engine (23), CLI contract (13) | 99 pass, 0 fail | Local Windows 11; CI on Windows, Ubuntu 22.04, macOS Apple Silicon, macOS Intel |
| Frontend view logic | 17 pass, 0 fail | Same |
| `cargo fmt --check`, `cargo clippy -D warnings` | Clean | Local and CI |
| `cargo audit` | 0 vulnerabilities, 2 upstream warnings with no fix | Local and CI |
| `npm audit` | 0 vulnerabilities | Local and CI |
| Secret scan (gitleaks, full history) | Pass, with an allowlist of nine named fake test credentials | CI |
| CI run | Green: https://github.com/lumilumiere/quotis/actions/runs/43748060541 (commit `ada67f5`) | GitHub Actions |
| Desktop app, by hand through the WebView debugging port | Collapse / expand / Escape, Island / Notch / Panel, dark / light, glass, pinned provider, Settings window, hostile-string rendering: all as expected | Local Windows 11 only, fixture data |
| Agent skill | 4 of 4 scenarios pass in Claude Code | Local |
| Windows installers | NSIS and MSI build. **Not installed or run** | Local |

## Independent reviews

| Review | State |
|---|---|
| 0001 baseline (Codex) | Done. 18 findings, none critical or high |
| 0002 implementation at `f0b3ea5` (Codex) | Done. 16 new findings, none critical or high. Fixes in `cbcdaa0`, dispositions in `SECURITY_REVIEW.md` |
| 0003 verification of those fixes (Codex) | **Not done.** Codex reached its usage limit partway through and wrote no report. The fixes are therefore verified only by the implementer's own tests |

## Blockers

Each needs the owner or something this session did not have.

1. **Fix verification by Codex (review 0003).** Re-dispatch when Codex's limit resets. The task spec is in the Orca run (`task` for dispatch `ctx_a1f2fcffe61c`).
2. **No live-provider check.** No provider has been run against a real account in 2.0. Running the CLI against the real logins on this machine was blocked by the session's permission system. To do it: `quotis-cli status --live`, compare with each tool's own usage screen, record in `PROVIDER_MATRIX.md`.
3. **macOS and Linux desktop behaviour.** CI proves the code builds and the tests pass there. Nobody has seen the overlay run on either.
4. **Installer smoke test.** Not run here because installing would replace any Quotis already installed on this machine.
5. **Code signing.** No certificates exist. Builds are unsigned.
6. **Agensi submission.** Needs a seller account and payout setup. Fees and payout terms were not read.
7. **Legal and naming.** Risk R1 (four providers use undocumented endpoints), R12 (no trademark search for "Quotis"), and the draft licence and privacy policy all need the owner's review.

## Things that happened that you should know

- **Two synthetic mouse clicks went to League of Legends.** While testing click-to-expand, two left clicks were sent at the top centre of the primary screen; the game was in the foreground and received them. Input injection was stopped immediately and not used again.
- **The branch was pushed to the public repository** so CI could run on macOS and Linux.
- **Codex's update prompt was dismissed twice** with "skip until next version"; nothing was installed globally.
- **Rust toolchain components added:** `clippy`, `rustfmt`, and the `cargo-audit` tool.
- **A download-and-run of gitleaks locally was blocked** by the permission system; history was searched with `git` instead and gitleaks runs in CI.

## Remaining risks

See [RISK_REGISTER.md](RISK_REGISTER.md). The ones that matter most: R1 (undocumented endpoints), R3 (unsigned), R4 (macOS untested on hardware).

## Next action

Re-run the Codex fix verification (blocker 1), then the live-provider check (blocker 2). Everything else in [DEVELOPMENT_ROADMAP.md](DEVELOPMENT_ROADMAP.md) "To reach a 2.0 release" follows.
