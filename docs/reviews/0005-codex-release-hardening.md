# Codex release hardening

Review date: 2026-11-16. Original target: `fe4c1ef1d46d4701cb38ac5d882931dde6110053`.
Starting HEAD: `f435ec1ddada4b84cfaef287a736d35e06a24724`, documentation only after the target.
Clean worktree confirmed; original commits remain in Git. All four earlier review reports
and the six requested project documents read. No v0.2.1 re-audit. Codex only.

Original-target references below refer to `fe4c1ef`. Final verification and challenge-review
results are recorded separately. A source interleaving is not a reproduced native UI race.
Bare core module names refer to `crates/quotis-core/src/`; provider paths are under its
`providers/` directory. `main.rs` in shell findings means `src-tauri/src/main.rs`.

## Review and implementation sequence

1. Reproduce settings backup/main failure, corruption recovery and unsafe default enablement
   in `settings.rs`; preserve the committed generation even when preparing its replacement fails.
2. Exercise duplicate old-generation reports, hung local work and source-error provenance in
   engine integration tests; bound issued work across reconfiguration and add a one-shot deadline.
3. Exercise unknown telemetry, shared-prefix rewrites and listing errors; propagate coverage
   and cap total filesystem enumeration work.
4. Add maintained shell save/drag/quit and renderer coverage where feasible, then rerun all
   component and workspace checks in an isolated fixture environment.
5. Obtain a fresh-context Codex challenge review with old/new commits and raw test evidence.
6. Build Windows artifacts, perform isolated runtime/performance checks, inspect security and
   free skill packaging, push only the development branch and verify cross-platform CI.
7. Record separate PASS/FAIL/UNVERIFIED decisions and remaining human-only checks.

## Initial evidence

Both target CI runs succeeded: `43809979978` (code) and `43810459981` (documentation).
These historical results are not final validation. Real provider credentials were denied to
the previous session; that boundary is preserved. Only fixtures will be read in this run.

Original-target release-blocking determination: **YES**. Final source disposition follows
the verification record; public release remains FAIL under separate runtime/account gates.

## Missing review of fe4c1ef

| Finding | Severity / blocker | Original evidence and reproduction | Remedy and regression |
|---|---|---|---|
| N02: prepared backup becomes committed recovery state | Medium / YES | `crates/quotis-core/src/settings.rs:136` writes candidate backup before primary at `:137`; load at `:126` accepts that backup. Save disabled Codex, replace synthetic primary with a directory, attempt enabling save, then load/corrupt primary/load. Failed save restores enabled defaults. Executed maintained regression failed on original code. | An atomic commit record selects the committed generation. Prepare both compatibility copies, replace commit record last. On failure try rollback; recovery ignores prepared files even when rollback fails. `failed_primary_write_never_commits_the_prepared_backup`, `failed_commit_and_failed_rollback_keep_the_previous_generation`. |
| N02: total corruption enables default providers | Medium / YES | `settings.rs:126` defaults when both copies fail parsing. Write corrupt primary and backup; loaded settings enable default login providers. Regression failed for Claude first; no real credentials accessed. | Distinguish absent first run from failed recovery; all registry providers disabled on failed recovery, with a credential-free stderr diagnostic. `unrecoverable_existing_settings_do_not_enable_providers`. |
| S01: movement during save | Original repair VERIFIED for in-memory state; remaining Medium / YES on normal exit | `src-tauri/src/main.rs:327` preserves current position and dirty mark; disk still contains previous position until the 15-second ticker (`:280`). `quit` at `:368` exits without flushing. Source interleaving: move during/after save, quit before next tick, restart at old position. No successful baseline WebView execution: debugging connection failed, recorded below. | Preserve current position and dirty mark, flush after pending serialized saves on normal quit and ExitRequested. Four maintained shell transaction tests added, including controlled movement during actual disk write and explicit exit flush. Forced termination/power loss remains outside the normal-exit guarantee. |
| S02: global file cap | Original repair VERIFIED, overall visit budget still missing | `fsx.rs:44` checks full file cap before descending. Stops later sibling traversal once matching-file budget is exhausted. Existing fixture assertions do not count visits. Empty/nonmatching wide trees still have no total bound (`:34`). | Add a shared 40,000-entry budget, including unsuccessful entries, across all recursion. `global_budget_stops_a_tree_with_no_matching_files` exercises a small injected budget. |
| R05: duplicate old completion releases unrelated workers | Medium / YES | `engine.rs:316` decrements outstanding before generation rejection; no job identity is consumed. Issue three abandoned polls, reconfigure, report one old job, start replacement, report same old job repeatedly. Regression failed: more jobs issued. | Track outstanding sequence IDs, remove once before generation checks. Duplicate completions ignored. `duplicate_old_reports_do_not_release_other_workers`. |
| R05: local work has no cap | Medium / YES | `engine.rs:224` issues local jobs without accounting; settings apply at `main.rs:188` starts another detached probe/local thread every time. Twenty reconfigurations issue twenty unfinished local jobs; regression failed 20 vs 3. | Cap local jobs at three/provider across configuration generations; serialize/coalesce shell discovery in one background worker. `local_workers_stay_bounded_through_reconfiguration`. Arbitrarily stuck OS calls are capped, not terminated. |
| N04: local error provenance is lost through online failure | Medium / YES | `engine.rs:389` replaces local error with online error, then online success clears it at `:374`. Local PermissionDenied -> online Offline -> online success yields no error. Maintained regression failed. | Separate local and online errors; local success/absence clears local error only. `local_failure_survives_online_failure_then_success`. Both errors retained internally; snapshot prioritizes local error when both exist. |
| N04: unknown formats still certify fresh zero | Medium / YES | `providers/tokenlog.rs:150` only counts exact `"tokens"` markers. Nonempty `not json`, `usage.total_tokens` or `tokenCount` schema reads zero without qualification. Regression failed on original. | Qualify unknown/non-JSON lines while allowing known non-usage record types; preserve coverage state across passes. `unknown_nonempty_log_is_not_an_unqualified_zero`. |
| N04: failed listing silently appears complete | Medium / YES | `fsx.rs:34` returns silently on read_dir error; `:35` flattens per-entry errors; `:48` drops file_type errors. Test regular-file-as-directory returned complete listing. Partial iterator failure source path separately confirmed. | Propagate incompleteness for read_dir, iterator, file type and metadata errors; token totals remain qualified. `enumeration_failure_is_incomplete`, `an_error_partway_through_enumeration_is_not_flattened_away`. |
| N03: ordinary shared-header replacement not detected | Medium / YES for trustworthy local usage | `fsx.rs:113` accepts longer replacement sharing first 64 bytes. Write >100-byte common header plus 10, read, rewrite header plus 900 and extra line: not restarted. A replaced file with matching size/mtime/header/tail also not restarted. Both maintained probes failed. | Keep an OS file identity handle plus consumed-boundary fingerprint, size and mtime. Test both in-place growth rewrite and distinct file identity with preserved metadata. In-place adversarial changes outside samples with preserved identity/mtime remain a stated limitation. |
| N10: live local collection has no overall deadline | Medium / YES for agent-facing CLI | `engine.rs:461` runs local jobs inline, `:468` scoped online join waits for all. Synthetic local collector sleeps 300 ms; unbounded original call exceeds 200 ms regression threshold. Infinite wait follows the same branch; no uncontrolled permanent worker created for reproduction. | Shared 60-second wait deadline around detection/local/online jobs. Injectable shorter deadline for deterministic tests. Incomplete workers retain their slots and expose Unavailable. `one_shot_local_collection_has_an_overall_deadline` now uses 30 ms deadline against same 300 ms collector. |
| N10: Retry-After dates ignored and long delays shortened | Medium / YES for respecting provider minimum | `net.rs:20` parses integers only; `engine.rs:384` caps a week. Loopback HTTP-date 2030 fixture yields None; scripted u64::MAX delay becomes 604800. Regressions failed. | Parse HTTP dates with `httpdate`, round fractional delay up, preserve numeric delay without cap, saturate schedule arithmetic; manual/all-online respect backoff. `retry_after_http_date_is_honoured`, `retry_after_is_not_shortened_even_beyond_a_week`. |
| N16: no automated desktop shell or real renderer coverage | Low / assurance gap | `src-tauri/src/main.rs` had zero tests. JS suite covered formatting only. | Extract production persistence coordination and test actual write/apply/publication interleavings. Windows CDP smoke script targets only its own fixture process; execution status below. These tests do not certify native monitor behavior. |

No critical/high exploit established. The settings/privacy, usage-integrity, worker-accounting
and CLI-deadline findings block public release until repaired and verified.

## Required scenario coverage

1. Backup succeeds / main fails: reproduced by blocking only primary replacement.
2. Failed save followed by primary corruption: same regression performs subsequent corruption.
3. Unintended provider restoration: failed candidate enables previously disabled Codex; asserted recovery stays disabled.
4. Immediate exit after movement/save: source-derived missing exit flush; maintained shell interleaving test plus planned native smoke.
5. Repeated reconfiguration with hung work: online cap survives target reconfiguration; new local cap regression initially fails.
6. Multiple old-generation reports: reproducible slot over-release; sequence membership repair.
7. Unknown log formats: three nonempty malformed/changed formats, two successive passes.
8. Mid-directory enumeration error: injected real DirEntry iterator with an Err between two entries; coverage preserved.
9. Blocking local live collector: 300 ms sleeper with 30 ms caller deadline; timed-out worker remains accounted.
10. Concurrent settings operations: production coordinator test stalls first publication while second save waits; disk/runtime/emitted order agrees.

## Verification record so far

- Original regressions: core **67 pass / 5 fail**; engine **25 pass / 2 fail**.
- Additional local-cap/deadline/retry probes: engine **27 pass / 3 fail**, HTTP-date **0 pass / 1 fail**, identity **0 pass / 1 fail**. Expected failures recorded before repairs.
- After initial repairs: core **75 pass / 0 fail**, engine **30 pass / 0 fail**, CLI **13 pass / 0 fail**, shell **4 pass / 0 fail**; JS **17 pass / 0 fail**. Additional traversal tests added afterwards; final totals belong in the final gate record.
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: pass locally.
- `npm audit`: zero reported vulnerabilities. `cargo audit`: zero vulnerabilities; warnings RUSTSEC-2024-0370 (unmaintained proc-macro-error) and RUSTSEC-2024-0429 (Linux glib unsound iterator). The latter has fixes in newer glib, unavailable within Tauri's current GTK dependency line; it is not proof of harmlessness.
- Baseline release binary smoke attempts reached fixture snapshots (2.5-3.4 s) but CDP discovery returned ECONNREFUSED. No baseline screenshots or successful native drag reproduction claimed. Each attempt killed only its own fixture child and preserved its report under `%TEMP%/quotis-desktop-smoke-*`.
- All Rust executions redirect home/config/provider defaults and remove API-key variables through `scripts/with-fixtures.ps1`; loopback HTTP only. No live credentials, provider requests, installers, Claude sessions or charges.

## Challenge review and final repairs

Fresh-context Codex received original reports, exact old/new commits and release criteria,
not only this implementer's conclusions. Its [unaltered challenge history and appended
rechecks](0005-fresh-context-challenge.md) record five more medium blockers at `dfdcfe6`:
text CLI dropped qualifiers; a known `info` type hid an unfamiliar usage field; diagnose
re-probed after its deadline; Retry-After began at issuance; worker exhaustion looked absent
after reconfiguration. All five were reproduced and repaired, then independently rechecked.
`providers --live` now rejects before discovery; standalone discovery remains synchronous.

The reviewer also corrected the FREE skill's money/request projection normalization. This
is another Codex context, not independent review by another implementation/model family.
No Claude invoked. Reviewer made no implementation edits and has no background work left.

Additional implementation evidence:

- Aggregate token retention: a synthetic 300,000-pair state exceeded the proposed 200,000
  cap (regression failed); repaired aggregate budget passes with persistent qualification.
  This bounds retained pairs to 3.2 MB (about 3.05 MiB), not the full process or transient parse memory.
- Overlay partial zero: actual formatter returned plain 0; new test initially 17 pass / 1 fail.
  Repaired row/pill show `≥0` with partial lower-bound detail; 18 JS tests pass. Fresh reviewer
  verified it and recorded two low caveats: mixed partial/complete quota ranking (no current
  producer) and specific cached-local error omitted from pill title but retained in card.
- Windows CI at `72e400c` failed one shell test with AccessDenied; parallel fixtures used
  PID+timestamp only and could collide on coarse clocks. `059ff3f` adds an atomic sequence;
  four local shell tests and all six CI jobs then passed. Collision is consistent with the
  observed failure; no assertion was removed and no local OS clock failure is invented.
- Production runtime on `72e400c`: startup/snapshot, refresh, exposed file-read error and
  exit 0 observed, but a targeted window move was not persisted. `fe4c1ef` source at
  `src-tauri/src/main.rs:234` ignores any movement within 400 ms of app placement. Extracted
  unchanged rule plus `movement_immediately_after_placement_is_recorded_before_exit`
  failed (None vs 170,110). `a99290e` replaces the timer with the requested destination
  (`main.rs:60`, `:235`); stale queued events are rejected at `:247`. Immediate/drag-back
  tests and six shell tests pass. Fresh reviewer checked these paths and lock ordering.
  It records a low source-only edge if native set_position fails and leaves a marker.

## Final source verification

Source/test commit `a99290e37365b09df1435712a5d6afa7d5df91d9`:

| Gate | Observed result |
|---|---|
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | PASS |
| `cargo test --workspace --locked` | **130 pass, 0 fail**: 78 core + 32 engine + 14 CLI + 6 shell; 0 ignored |
| `npm test` | **18 pass, 0 fail**, 0 skipped |
| `npm run build` | PASS |
| `npm audit` | PASS, 0 vulnerabilities |
| `cargo audit` |0vulnerabilities,2upstream warnings; exit 0 |

Native artifacts, final CI, performance and separate distribution decisions are recorded in
[RELEASE_READINESS_FINAL.md](../RELEASE_READINESS_FINAL.md). No available source blocker
remains from this reviewed scope; low caveats and platform/authorization limits stay open.

Final-source CI [43821955837](https://github.com/lumilumiere/quotis/actions/runs/43821955837):
six jobs PASS. Windows ran 130 Rust tests; Linux and both macOS runners each ran 131 (extra
Unix permission test), all zero failures/ignored. Each runner passed 18 frontend tests.
Full-history gitleaks passed. Both unsigned Windows installers were built and retained.
The final production runtime rerun completed all three modes, snapshot/normal-close/mode
persistence, and immediate native move persistence for Island/Panel. It exposed a forced
local-read error while retaining last values. [Raw evidence](../validation/windows-runtime-a99290e.json)
and [performance method/results](../PERFORMANCE_REPORT.md) are preserved. N16 remains
partial: maintained shell tests now exist; WebView debugger failure prevented a successful
new DOM click-through, and native visual/platform certification is still unavailable.

**Final public release-blocking determination: YES.** Native macOS/Linux, full Windows
interaction/install/upgrade, real provider comparisons and signing remain UNVERIFIED.
No tag, main merge, public binaries or marketplace publication. A passing source suite or
fresh-context Codex verdict is not a public release authorization.
