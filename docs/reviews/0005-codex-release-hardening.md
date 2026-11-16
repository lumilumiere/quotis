# Codex release hardening

Review date: 2026-11-16. Original target: `fe4c1ef1d46d4701cb38ac5d882931dde6110053`.
Starting HEAD: `f435ec1ddada4b84cfaef287a736d35e06a24724`, documentation only after the target.
Clean worktree confirmed; original commits remain in Git. All four earlier review reports
and the six requested project documents read. No v0.2.1 re-audit. Codex only.

Original-target references below refer to `fe4c1ef`. Final verification and challenge-review
results are recorded separately. A source interleaving is not a reproduced native UI race.

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

Release-blocking determination: **YES pending the reproductions and repairs below.**

## Missing review of fe4c1ef

| Finding | Severity / blocker | Original evidence and reproduction | Remedy and regression |
|---|---|---|---|
| N02: prepared backup becomes committed recovery state | Medium / YES | `crates/quotis-core/src/settings.rs:137` writes candidate backup before primary at `:138`; load at `:126` accepts that backup. Save disabled Codex, replace synthetic primary with a directory, attempt enabling save, then load/corrupt primary/load. Failed save restores enabled defaults. Executed maintained regression failed on original code. | An atomic commit record selects the committed generation. Prepare both compatibility copies, replace commit record last. On failure try rollback; recovery ignores prepared files even when rollback fails. `failed_primary_write_never_commits_the_prepared_backup`, `failed_commit_and_failed_rollback_keep_the_previous_generation`. |
| N02: total corruption enables default providers | Medium / YES | `settings.rs:126` defaults when both copies fail parsing. Write corrupt primary and backup; loaded settings enable default login providers. Regression failed for Claude first; no real credentials accessed. | Distinguish absent first run from failed recovery; all registry providers disabled on failed recovery, with a credential-free stderr diagnostic. `unrecoverable_existing_settings_do_not_enable_providers`. |
| S01: movement during save | Original repair VERIFIED for in-memory state; remaining Medium / YES on normal exit | `src-tauri/src/main.rs:326` preserves current position and dirty mark; disk still contains previous position until the 15-second ticker (`:280`). `quit` at `:370` exits without flushing. Source interleaving: move during/after save, quit before next tick, restart at old position. No successful baseline WebView execution: debugging connection failed, recorded below. | Preserve current position and dirty mark, flush after pending serialized saves on normal quit and ExitRequested. Four maintained shell transaction tests added, including controlled movement during actual disk write and explicit exit flush. Forced termination/power loss remains outside the normal-exit guarantee. |
| S02: global file cap | Original repair VERIFIED, overall visit budget still missing | `fsx.rs:44` checks full file cap before descending. Stops later sibling traversal once matching-file budget is exhausted. Existing fixture assertions do not count visits. Empty/nonmatching wide trees still have no total bound (`:32`). | Add a shared 40,000-entry budget, including unsuccessful entries, across all recursion. `global_budget_stops_a_tree_with_no_matching_files` exercises a small injected budget. |
| R05: duplicate old completion releases unrelated workers | Medium / YES | `engine.rs:316` decrements outstanding before generation rejection; no job identity is consumed. Issue three abandoned polls, reconfigure, report one old job, start replacement, report same old job repeatedly. Regression failed: more jobs issued. | Track outstanding sequence IDs, remove once before generation checks. Duplicate completions ignored. `duplicate_old_reports_do_not_release_other_workers`. |
| R05: local work has no cap | Medium / YES | `engine.rs:226` issues local jobs without accounting; settings apply at `main.rs:185` starts another detached probe/local thread every time. Twenty reconfigurations issue twenty unfinished local jobs; regression failed 20 vs 3. | Cap local jobs at three/provider across configuration generations; serialize/coalesce shell discovery in one background worker. `local_workers_stay_bounded_through_reconfiguration`. Arbitrarily stuck OS calls are capped, not terminated. |
| N04: local error provenance is lost through online failure | Medium / YES | `engine.rs:387` replaces local error with online error, then online success clears it at `:375`. Local PermissionDenied -> online Offline -> online success yields no error. Maintained regression failed. | Separate local and online errors; local success/absence clears local error only. `local_failure_survives_online_failure_then_success`. Both errors retained internally; snapshot prioritizes local error when both exist. |
| N04: unknown formats still certify fresh zero | Medium / YES | `providers/tokenlog.rs:151` only counts exact `"tokens"` markers. Nonempty `not json`, `usage.total_tokens` or `tokenCount` schema reads zero without qualification. Regression failed on original. | Qualify unknown/non-JSON lines while allowing known non-usage record types; preserve coverage state across passes. `unknown_nonempty_log_is_not_an_unqualified_zero`. |
| N04: failed listing silently appears complete | Medium / YES | `fsx.rs:33` returns silently on read_dir error; `:34` flattens per-entry errors; `:50` drops file_type errors. Test regular-file-as-directory returned complete listing. Partial iterator failure source path separately confirmed. | Propagate incompleteness for read_dir, iterator, file type and metadata errors; token totals remain qualified. `enumeration_failure_is_incomplete`, `an_error_partway_through_enumeration_is_not_flattened_away`. |
| N03: ordinary shared-header replacement not detected | Medium / YES for trustworthy local usage | `fsx.rs:110` accepts longer replacement sharing first 64 bytes. Write >100-byte common header plus 10, read, rewrite header plus 900 and extra line: not restarted. A replaced file with matching size/mtime/header/tail also not restarted. Both maintained probes failed. | Keep an OS file identity handle plus consumed-boundary fingerprint, size and mtime. Test both in-place growth rewrite and distinct file identity with preserved metadata. In-place adversarial changes outside samples with preserved identity/mtime remain a stated limitation. |
| N10: live local collection has no overall deadline | Medium / YES for agent-facing CLI | `engine.rs:466` runs local jobs inline, `:474` scoped online join waits for all. Synthetic local collector sleeps 300 ms; unbounded original call exceeds 200 ms regression threshold. Infinite wait follows the same branch; no uncontrolled permanent worker created for reproduction. | Shared 60-second wait deadline around detection/local/online jobs. Injectable shorter deadline for deterministic tests. Incomplete workers retain their slots and expose Unavailable. `one_shot_local_collection_has_an_overall_deadline` now uses 30 ms deadline against same 300 ms collector. |
| N10: Retry-After dates ignored and long delays shortened | Medium / YES for respecting provider minimum | `net.rs:20` parses integers only; `engine.rs:385` caps a week. Loopback HTTP-date 2030 fixture yields None; scripted u64::MAX delay becomes 604800. Regressions failed. | Parse HTTP dates with `httpdate`, round fractional delay up, preserve numeric delay without cap, saturate schedule arithmetic; manual/all-online respect backoff. `retry_after_http_date_is_honoured`, `retry_after_is_not_shortened_even_beyond_a_week`. |
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

## Remaining limits before final gates

No credential authorization, native macOS/Linux environment, clean Windows VM or signing
certificate is available. No release tag, main merge or marketplace publication is authorized
by a FAIL decision. Fresh-context Codex challenge review and final repaired-build measurements
are still required; results will be appended before delivery.
