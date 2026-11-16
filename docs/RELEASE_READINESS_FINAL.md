# Quotis 2.0 final release readiness

2026-11-16. **Public release: FAIL.** Source repairs and available checks are complete;
mandatory native/account/install evidence is still missing. UNVERIFIED is not PASS.

Source/test commit: `a99290e37365b09df1435712a5d6afa7d5df91d9`, branch `quotis-2.0`, pushed.
Later report commits change documentation/verification scripts only. Original `fe4c1ef`
and `f435ec1` remain in the graph. Working implementation was clean at takeover; no code
followed `fe4c1ef` until this hardening run. No main merge, history rewrite, force-push,
tag, public release, installer publication, Claude invocation or paid service purchase.

## Separate decisions

| Distribution / scope | Decision | Evidence and outstanding gate |
|---|---|---|
| Source code | **PASS** | Reproduced/fixed blockers, 130 local Rust tests, 18 JS tests, fresh-context Codex challenge/rechecks, all six CI jobs green. Scoped assessment, not comprehensive security certification |
| Windows desktop | **UNVERIFIED** | Final executable/NSIS/MSI built; all three launch modes passed fixture collection, normal close and persistence; Island/Panel moves persisted. Full visual interaction, DPI/multiple-monitor and clean installation/upgrade not certified |
| macOS desktop | **UNVERIFIED** | ARM and Intel CI tests pass; no native overlay, Spaces/glass, install/sign/notarize or Keychain prompt/denial execution |
| Linux desktop | **UNVERIFIED** | Ubuntu CI tests/build pass; no X11/Wayland native session or package install/remove |
| Provider monitoring | **UNVERIFIED** | All nine adapters fixture-tested, none compared with an authorized account's actual usage display. Prior credential denial preserved |
| CLI | **PASS** | 14 contract tests, read-only fixture checks, bounded live collector waiting, qualified text/JSON, actual sealed `status --live` invocation. Real-provider accuracy remains UNVERIFIED; standalone discovery/settings/output are not globally timed |
| Agensi skill package | **PASS** | Local text-only archive, ten files, validator and four offline scenario groups pass. FREE copy discloses unreleased/live-unverified dependency; actual marketplace delivery and owner licence/privacy sign-off remain UNVERIFIED |
| Public release | **FAIL** | Required Windows/native-platform/account/install gates remain UNVERIFIED; unsigned artifacts and owner legal/naming review unresolved |

## Repairs and review evidence

[Review 0005](reviews/0005-codex-release-hardening.md) records original file/line references,
reproductions, red/green results and dispositions. [Fresh-context challenge](reviews/0005-fresh-context-challenge.md)
preserves the reviewer's original five blockers and exact-commit rechecks, including later
overlay/movement fixes. This was another Codex context; it is not an independent review by
another implementation/model family. No conclusion was accepted solely because tests passed.

Completed changes:

- Atomic authoritative settings commit record; prepared backup/main failures and failed
  rollback cannot become committed recovery state. Corrupt existing state disables providers.
- Serialized settings write/apply/publication, normal-exit position flush and explicit native
  placement tracking instead of the 400ms movement-discard heuristic.
- Sequence-set worker accounting across reconfiguration and duplicate reports, local/online
  caps, exhausted-worker errors, separate persistent source-error provenance.
- Unknown/malformed telemetry and listing errors retain lower-bound qualifications; global
  traversal and retained-record budgets, OS file identity and append-boundary checks.
- Shared live-collector deadline, HTTP-date Retry-After, unshortened delays measured from
  response receipt, and no second synchronous diagnose probe. Ignored providers/live rejected.
- Partial totals visible in CLI text/JSON and overlay, including `≥0` rather than plain zero.

Known limited concerns remain in RISK_REGISTER.md: arbitrary OS I/O is capped rather than
cancelled; preserved-metadata in-place rewrites outside sampled bytes can evade detection;
snapshot authenticity relies on local file permissions; native move API failures can leave
a destination marker; pill tooltip error detail and hypothetical mixed partial quota ranking
have low caveats. Upstream GTK/build dependency warnings remain. These are not hidden PASSes.

## Final checks

All commands adapted to this workspace. Rust local runs used the sealed fixture wrapper,
with home/config/provider defaults redirected and API-key variables removed. No real logins
or tool conversations were inspected. Network tests used synthetic loopback HTTP fixtures.

| Check on final source | Result |
|---|---|
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | PASS |
| `cargo test --workspace --locked` (local Windows) | **130 passed, 0 failed, 0 ignored**: core 78, engine 32, CLI 14, shell 6 |
| `npm test` | **18 passed, 0 failed, 0 skipped** |
| `npm run build` | PASS; both entry points |
| `npm audit` | 0 vulnerabilities |
| `cargo audit` | 0 vulnerabilities, 2 warnings: RUSTSEC-2024-0370 and RUSTSEC-2024-0429; exit 0 |
| Full-history secret scanning | PASS in final-source CI; no denied local scanner download retried |
| Windows production build | PASS, executable plus NSIS/MSI; unsigned |
| Offline skill checks | 10 text files; four scenario groups passed; no installer in ZIP |

Final-source [CI run 43821955837](https://github.com/lumilumiere/quotis/actions/runs/43821955837)
on `a99290e`: **six jobs passed** (lint, audit, four platform tests). Downloaded job logs
confirm Windows 130 Rust tests; Ubuntu/macOS ARM/macOS Intel each 131 (one extra Unix core
permission test); all zero failed/ignored. Each platform ran 18 JS tests successfully.
CI compiles/checks native code but does not certify visible desktop compatibility.

An earlier CI run on `72e400c` failed one Windows shell fixture with AccessDenied (3 pass,
1 fail). PID+timestamp fixture naming was not collision-safe under coarse clocks; an atomic
counter was added without removing assertions. Subsequent `059ff3f` and final-source CI passed.
No unexplained failed final gate is being called successful.

## Windows artifacts and measured runtime

Local files: `target/release/quotis.exe`, `target/release/quotis-cli.exe`,
`target/release/bundle/nsis/Quotis_2.0.0_x64-setup.exe`,
`target/release/bundle/msi/Quotis_2.0.0_x64_en-US.msi`.
[SHA256 manifest](WINDOWS_ARTIFACTS_SHA256SUMS.txt). Desktop binary hash:
`E98B67BB581DC2C6C58114662EA7E93222459630E43860EEAEB6B45D58610142`.
Installers were built and retained, **not installed**, not uploaded or released.

[Final production runtime data](validation/windows-runtime-a99290e.json): Island/Notch/Panel
startup to fixture snapshot 3,238 / 3,732 / 2,338 ms, all exit 0; saved modes and targeted Island/Panel
moves persisted. Idle tree 144.00 MiB private memory, 0.517% of one core over 60.39 s. Forced local
read failure kept 84-token values with Unavailable at 144.23 MiB. See
[PERFORMANCE_REPORT.md](PERFORMANCE_REPORT.md) for method, refresh cost and limitations.
The prior approximately 145 MB result is consistent; 120 MB is not an absolute gate.

Four earlier WebView debugger attempts (including a debug build) reached snapshots but
discovery returned ECONNREFUSED. No new successful DOM click-through or screenshots are
claimed. The final runtime script uses only its own child HWND/process, without global
input. It verifies state transitions/data, not visual appearance. Only one display was
available; no OS display/scaling changes were made.

One validation debug build failed when drive D filled. Policy rejected direct recursive
incremental-cache deletion; `cargo clean -p quotis --profile dev` safely reclaimed generated
outputs while preserving source/release files. A first isolated-environment NSIS attempt
panicked inside the bundler; normal-environment builds subsequently produced both installers.
The actual failure results are retained rather than counted as passes.

## Provider verification

`quotis-cli status --live` was executed under a sealed empty fixture profile and returned
"No providers connected", exit 0. This verified command execution without reading real
credentials; it is **not** a live account test or provider-display comparison. Positive local
fixtures and error conditions are covered by maintained tests/runtime data.
[PROVIDER_MATRIX.md](PROVIDER_MATRIX.md) records all nine providers as live-UNVERIFIED,
with sources, reset-window limitations and exact authorized-account comparison instructions.
No unsupported provider is represented as working.

## FREE Agensi artifact

The ignored workspace exists as a separate local repository, commit
`718f848dca9e5a3d902b60406d4cb116d957f56f`; it was not pushed. Rebuilt archive:
`commercial/dist/quotis-ai-usage-intelligence-1.0.0.zip`, **19,970 bytes**, ten text files.
SHA256: `449EEF1798A41B65C110EB5A716531613B4913C5445DA23B027B34364E4A9488`.
The original ZIP was preserved as `*-before-hardening.zip` locally.

`node commercial/validate.mjs` and `node scripts/validate-skill.mjs` pass. Offline groups:
capacity/freshness, normalized percent/money projections, expired historical data, and
permission/partial-output boundaries. These are deterministic checks and Codex content
review, not model reliability trials or an Agensi delivery test. No Claude scenarios rerun.
[FREE listing copy](AGENSI_FREE_LISTING.md) removes the former paid offer and discloses
historical samples, unreleased 2.0 dependency, fixture-only integrations and privacy boundaries.
No executable installer is in the package. No listing or account mutation was performed.

## Exact remaining human/environment tasks

1. Authorize named test accounts/local data and compare every advertised provider against
   its actual usage screen, recording timestamps/windows/errors. Stop on denial.
2. Complete Windows visual/keyboard/settings/stale/error checks and multiple-monitor/DPI
   tests on suitable hardware; the scoped runtime result is not that full test.
3. Run native macOS ARM/Intel overlay and separate Keychain prompt/deny/timeout checks;
   run Linux X11/Wayland desktop and packaging checks on real native sessions.
4. Test NSIS/MSI installation/removal and safe 0.2.1 upgrade in disposable VMs. Preserve
   evidence and do not replace the user's current installation.
5. Supply signing/notarization credentials only with explicit authorization, or make an
   explicit unsigned-distribution decision; resolve naming, privacy/licence and endpoint terms.
6. Use an authorized Agensi creator account to submit the reviewed ZIP as FREE with current
   disclosures, verify actual MCP delivery and complete marketplace review. No payout setup
   or paid purchase is needed for the prepared FREE listing.

[NATIVE_RELEASE_CHECKS.md](NATIVE_RELEASE_CHECKS.md) provides reproducible hardware/VM steps.
No background app/reviewer work is left running when this report is delivered. Do not merge
main or create a public tag/release while the decision remains FAIL.
