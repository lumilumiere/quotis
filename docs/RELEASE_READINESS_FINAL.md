# Quotis 2.0 final release readiness

Updated 2026-11-16 after final distribution validation. **Public desktop release: FAIL.**
UNVERIFIED is not PASS. Feasible source, native process/profile and package work is complete;
remaining tasks require a disposable OS environment, hardware, authorized accounts or owner decisions.

Final production source: `4718a9b545379d1dfd2799db2d56323bb1d354aa`, pushed to `quotis-2.0`.
This continues `2938e21`; repairs are `5df6cb0`, `1a0dc36`, `1ee544e`, `4718a9b`.
Later report/script commits do not alter production behavior. Main remains `f80bc4062ca589a79ef5dbb7a4f45b434b3aa3f7`.
No main merge, force-push, history rewrite, public tag/release, Claude invocation or paid purchase.

## Separate decisions

| Scope | Decision | Evidence / remaining gate |
|---|---|---|
| Source code | **PASS** | Confirmed repairs, red/green regressions, 131 local Windows Rust tests, 33 JS tests and six successful final-source CI jobs; scoped assessment with documented lifecycle/security limits |
| Windows desktop | **UNVERIFIED** | Final standalone and extracted MSI each passed 20 native assertions; all three modes, normal close, persistence/restart and resource sampling passed. Clean install/upgrade/uninstall, visual/keyboard/DPI, full stale/network behavior and stable OS coverage remain uncertified |
| macOS desktop | **UNVERIFIED** | ARM/Intel CI logic/build pass; no native overlay/install/sign/notarize or separate Keychain prompt/denial/timeout evidence |
| Linux desktop | **UNVERIFIED** | Ubuntu CI logic/build pass; no native X11/Wayland or package install/remove evidence |
| Provider monitoring | **UNVERIFIED** | Nine fixture-tested adapters; zero authorized real-account/provider-screen comparisons. Previous credential denial preserved |
| CLI | **PASS** | 14 contracts, isolated read-only live/offline check, unchanged state files, bounded collector wait and qualified output. Real-provider accuracy and arbitrary OS I/O remain outside this PASS |
| Agensi skill package | **PASS** | Ten text files exactly match archive; structure/content and four offline scenario groups pass; no executable installer |
| FREE Agensi publication | **UNVERIFIED** | Copy prepared, not uploaded. Draft licence, privacy identity/contact/date, creator authorization/profile, marketplace review and actual MCP delivery remain unresolved |
| Public desktop release | **FAIL** | Mandatory native/account/distribution gates above remain UNVERIFIED; all Windows artifacts unsigned; publisher/legal/naming decisions unresolved |

## Windows evidence

[Distribution report](DISTRIBUTION_VALIDATION.md), [structured observations](validation/windows-distribution-final.json),
[native reproduction instructions](NATIVE_RELEASE_CHECKS.md), [artifact hashes](WINDOWS_ARTIFACTS_SHA256SUMS.txt).
Actual host: Windows 11 Pro x64, build 26300.9818, one display. Tests used unique fixture
homes/provider directories, copied executables and dedicated WebView profiles. Only synthetic
Gemini data was enabled. Autostart was not toggled; native actions targeted the recorded
PID/HWND using UI Automation patterns, with no global input or focus bypass.

Forty final native assertions passed: expand/collapse, Settings open/close, provider and
rapid setting toggles, persisted controls, actual Connected/Issue/recovery text, qualified
unknown-log counts, last metrics retained during local read failure, normal Quit and settings
replica agreement. Restart preserved the tested settings. Isolated
`quotis-cli status --live --offline --json` exited 0 without changing any of four state files.
These are local synthetic checks, not real provider comparisons.

The existing owner installation stayed at 0.2.1 with the same executable SHA256 and registration;
no Quotis Run entry exists before/after. No host installer/upgrade/uninstall was run. MSI
administrative payload extraction exited 0 after checking its standard action allowlist; the
payload differed from standalone only at three Tauri bundle-type marker bytes and passed its
own 20 native assertions. Extraction does not certify installation.

Final local build produced executable/NSIS/MSI; the CLI release build also passed. All four
are **NotSigned**. MSI is machine-wide (ALLUSERS=1); NSIS currentUser. Publisher defaults to
`github`, requiring an owner decision. Desktop packages supply GUI only; CLI is separate.
WebView2 bootstrap can require a download when the runtime is absent. Offline installation
and stable Windows-version compatibility remain UNVERIFIED.

Sandbox was not started: C: had481,075,200 bytes free at preflight, below Microsoft's 1 GB
minimum; a repeated optional-feature WMI query also failed for insufficient disk space.
No owner files were deleted to reclaim space. Foreground-window captures were occluded and
rejected; no Quotis screenshots or full visual certification are claimed. ComboBox SetValue
attempts did not change values and are recorded as UNVERIFIED.

Final runtime: [raw measurements](validation/windows-runtime-4718a9b.json). First snapshot
Island/Notch/Panel: 738/426/419 ms; all exited 0 and preserved mode; targeted Island/Panel moves
persisted. Idle process tree 144.21 MiB private memory,0.466% of one core over 60.36s. Failure
path 144.28 MiB retained 84-token values with Unavailable. Refresh CPU delta was below the
observed counter resolution, not zero work. See [performance method/limits](PERFORMANCE_REPORT.md).

## Repairs and final checks

The original hardening is in [review 0005](reviews/0005-codex-release-hardening.md).
[review 0006](reviews/0006-windows-distribution-validation.md) records newly reproduced native
badge, event-order, discovery and concurrent-save defects with exact old/current references.
Desktop capture revisions preserve error ordering without changing CLI/cache schema. Settings
saves use immutable queued requests; explicit Close/Escape await pending saves. Forced app/
WebView termination and whole-app quit during unacknowledged drafts are not certified to drain
those drafts; slow-I/O VM lifecycle checks remain necessary.

A [separate Codex context](reviews/0005-fresh-context-challenge.md) challenged the first badge
repair, reproduced three problems and rechecked their exact repair commits. This is not a
review by another model family. Its original adverse findings are preserved. Two final
Close/Escape regressions were added afterward and reproduced before repair.

| Check | Final source result |
|---|---|
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | PASS locally and CI |
| `cargo test --workspace --locked` under sealed fixture homes | **131 Windows tests**, 0 failed/ignored: 78 core, 32 engine, 14 CLI, 7 shell |
| `npm test` | **33 passed**, 0 failed/skipped |
| `npm run build` | PASS, both entry points |
| `npm audit` | 0 vulnerabilities |
| `cargo audit` | 0 vulnerabilities;2 allowed upstream warnings (RUSTSEC-2024-0370, RUSTSEC-2024-0429), exit 0 |
| Full-history secret scanning | PASS in final-source CI; no previously denied scanner/credential access retried |
| Windows production bundles / CLI | PASS local builds; unsigned, not published |
| Native UI Automation | 20 standalone +20 extracted-MSI assertions, all PASS |
| Offline skill | 10 archive/source entries match;4 scenario groups PASS |

Final-source [CI 43861615221](https://github.com/lumilumiere/quotis/actions/runs/43861615221):
**all six jobs PASS** (lint, audit, Windows, Ubuntu, macOS ARM, macOS Intel). Downloaded logs
confirm 131 Rust tests on Windows and 132 on each Unix platform (extra Unix permission test);
each platform passed 33 JS tests, zero failures/ignored. CI does not certify visible UI behavior.
The separate final report/script commit is identified in Git and the completion message.

A formatting check initially reported two new Rust layouts; formatting was corrected before
final checks. An offline metadata probe could not download an uncached Android-only crate;
the workspace lock acquired only the direct serde edge, and applicable locked builds/tests
subsequently passed. Earlier native harness failures, corrected fixture checks and old-source
regressions are recorded in the distribution evidence; none were counted as final passes.

Security remains scoped: no new unsafe text sink or credential access, desktop-only ordering
metadata, inherited Windows ACLs, upstream GTK warnings, arbitrary stuck OS calls capped rather
than cancelled, and local-cache authenticity assumptions remain documented. Passing scanners
is not comprehensive security verification. [Security](SECURITY_REVIEW.md), [risks](RISK_REGISTER.md).

## Provider and FREE Agensi evidence

[Provider matrix](PROVIDER_MATRIX.md): all nine real integrations **UNVERIFIED**. No newly
authorized account/local data was supplied, so no real connection, usage display, matching
account timestamp or reset comparison was obtained. Local synthetic counts are not a substitute.

Ignored separate commercial repository: local branch `codex-free-distribution-validation`,
commit `2e2ae19b08668ca3c214447b29399d03b47fb023`; not pushed. Candidate:
`commercial/dist/quotis-ai-usage-intelligence-1.0.0.zip`, **20,415 bytes**, 10 text files,
SHA256 `CD4C89BABF8C748B9C8A6911E75848B960DE05DCC79FC9419896F9DD98F252C7`.
Previous ZIPs preserved. `node commercial/validate.mjs` and `node scripts/validate-skill.mjs`
pass; archive bytes match source. These are deterministic structure/arithmetic checks, not
marketplace delivery or model reliability certification. No Claude test was rerun.

[FREE listing](AGENSI_FREE_LISTING.md) discloses historical offline demo, unreleased separate
CLI, GUI-only installers, fixture/live limits and approved optional network/credential access.
Privacy boundaries and permission-denial instructions were corrected. No desktop installer
is bundled. Actual publication still requires the owner's licence/privacy/contact decisions
and an authorized creator session; no upload or account mutation was attempted.

## Exact remaining owner/environment actions

1. Provision a disposable Windows VM/Sandbox with adequate disk space and trusted WebView2.
   Test NSIS and MSI independently: actual install, launch, settings, quit, uninstall and
   registration cleanup; restore snapshots. Install a verified 0.2.1 in a safe VM and test
   upgrade/migration without touching the owner's working copy. Test slow saves/whole-app
   exit with Settings open. Retain installer hashes, logs and observations.
2. On suitable native hardware, perform visual/keyboard/theme/mode/DPI/multi-monitor,
   stale-age and network-failure checks. This host's preview-build/scoped UIA evidence does
   not establish stable Windows support. Execute native macOS ARM/Intel, separate Keychain
   prompt/deny/timeout and Linux X11/Wayland/package tests using NATIVE_RELEASE_CHECKS.md.
3. Explicitly authorize named provider test accounts/data; compare each advertised adapter
   with its own usage interface at matching UTC timestamps and record reset/source/errors.
   Stop on denial. Credentials must not be printed or repurposed.
4. Decide publisher identity, signing/notarization or an explicit unsigned-distribution
   policy; resolve naming, licence/privacy/contact/date and undocumented-endpoint terms.
   Signing credentials require separate explicit authorization.
5. Complete Agensi creator account/profile and owner legal decisions. Rebuild/revalidate the
   ZIP after edits, submit as **FREE**, retain disclosures, pass marketplace review and
   verify actual MCP delivery. No payout setup or paid purchase is needed for FREE under
   the checked [Agensi rules](https://www.agensi.io/docs/pricing-your-skill).

No background app, build or review work remains when this report is delivered. Main and the
owner installation are preserved. Do not create a public desktop release while gates fail.
