# Security review

Status of every security-relevant finding from the two independent Codex reviews, with the fix and the test that guards it. The reviews themselves are in [reviews/](reviews/).

| Review | Commit reviewed | Findings | Critical / High |
|---|---|---|---|
| [0001 baseline](reviews/0001-baseline-codex.md) | `f80bc40` (v0.2.1) | 18 (14 medium, 4 low) | 0 / 0 |
| [0002 implementation](reviews/0002-implementation-codex.md) | `f0b3ea5` | 16 new (13 medium, 3 low); of the 18 baseline findings 7 fixed, 10 partial, 1 not fixed | 0 / 0 |
| [0003 fix verification](reviews/0003-fix-verification-codex.md) | `cd8d662` | Of N01-N16: 6 verified fixed, 9 partial, 1 accepted risk judged reasonable. 7 new medium regressions (R01-R07). **Judged release-blocking** | 0 / 0 |

No finding in any review was rated critical or high. Review 0003 judged the fix set release-blocking on R01, R02/R06, R03/R04 and N02. Those were repaired afterwards in `1ab7658`; **that repair has not itself been independently reviewed.** Treat "Fixed" below as "fixed and tested by the implementer" unless the 0003 column says "Verified".

## Review 0003: regressions and partial fixes, and what was done

| ID | Codex 0003 | Now (`1ab7658`) | Guarding test |
|---|---|---|---|
| R01 | Clamping a future timestamp to "now" made a bad record look like the freshest reading | Fixed: a pass containing a reading more than 5 minutes in the future is rejected as a whole and reported; nothing is re-dated | `a_future_dated_local_reading_is_never_believed_however_often_it_is_reread` |
| R02 | A "lower bound" warning vanished on the next pass | Fixed: each file remembers missing coverage until the gap is older than every window | `a_partial_total_stays_marked_partial_on_later_passes` |
| R06 | The per-folder entry cap could silently drop the only recent log | Fixed: any cap hit during the walk (entries, depth, file count) marks totals as a lower bound | `walk_is_capped_and_ignores_other_files` |
| R03 | A failed settings save had already changed the running app; a failed drag save was not retried | Fixed: the file is written first and the app changes only if that succeeds; the drag mark is cleared only after a successful write | Checked by hand in the running app |
| R04 | Overlapping saves could announce an older one last | Fixed: the whole save (write, apply, detect, announce) runs under the save lock | Checked by hand: two overlapping saves announced in order, runtime, file and backup all agree |
| N02 | The backup was the save before last, so a just-made "off" could be lost | Fixed: the backup is a second copy of the latest save | `a_damaged_settings_file_falls_back_to_the_last_good_one` (now starts with the provider on) |
| R05 | Leases let jobs that hang every time pile up without limit | Fixed: at most 3 outstanding online jobs per provider; a written-off job is reported as "a check did not finish" | `a_collector_that_hangs_every_time_cannot_pile_up_jobs` |
| R07 | An unreachable folder was treated as gone and its numbers deleted | Fixed: "not there" and "could not check" are told apart; the latter is an error that keeps the last numbers | `existence_distinguishes_absent_from_unknown` |
| N01 | A failed cache write after a recent success waited for the heartbeat | Fixed: retried on the next emit | Shell code, no automated test |
| N03 | A same-length rewrite sharing its first 64 bytes was missed | Fixed: size unchanged with a new modification time also restarts the read | `a_same_length_rewrite_that_keeps_its_first_bytes_restarts` |
| N04 | An online success cleared a local error; an unreadable Codex log became "no data yet"; invalid token lines read as a fresh zero | First two fixed (`an_online_success_does_not_hide_a_local_failure`). **Invalid token lines are still skipped silently**: a tool that changes its log format reads as zero usage (risk R7) | |
| N05 | See R01 | Fixed with R01 | |
| N08 | No total work budget across the whole walk | **Partial.** Per-folder, depth and file caps now report truncation; there is still no overall time budget | |
| N10 | `Retry-After` over a day was shortened; HTTP dates unparsed; no overall deadline; Keychain call has no timeout | `Retry-After` honoured up to a week. **HTTP dates, an overall deadline and a Keychain timeout are not done** | `rate_limit_backs_off_and_keeps_last_numbers` |
| N11 | A job past its lease was still accepted if no newer one had started | **Accepted**: such an answer is real data, and accepting it clears the timeout message. No result older than a newer job is ever accepted | |
| N16 | No test drives the real DOM renderer; shell races have no automated test | **Not done.** Covered only by the by-hand WebView checks | |
| N06, N07, N09, N13, N14, N15 | Verified fixed by Codex | Unchanged | |
| N12 | Accepted risk judged reasonable by Codex | Unchanged | |

## Review 0002: new findings

| ID | Finding | Disposition | Fix | Guarding test |
|---|---|---|---|---|
| N01 | An older snapshot could reach the UI or the snapshot file after a newer one | Fixed | Snapshot is taken and published under one `PUBLISH` lock; the "written" mark is set only on a successful write | Shell code; no automated test (the desktop shell has none) |
| N02 | Drag-position save bypassed the save lock; a damaged settings file re-enabled providers | Fixed | One `persist()` path that reads settings after taking `SAVING`; last-good `settings.json.bak` is used when the main file is damaged | `settings::a_damaged_settings_file_falls_back_to_the_last_good_one` |
| N03 | A replaced log of equal or greater length was read as "nothing new" or as an append | Fixed, with a stated ceiling | Checkpoints fingerprint the first 64 bytes; a change restarts the read. Oversized appends are flagged and the metric is marked partial | `fsx::a_replaced_file_of_the_same_or_greater_length_restarts`, `tokenlog::a_replaced_log_of_equal_length_is_recounted…` |
| N04 | Local collection failures vanished once data existed; a removed source left old numbers | Fixed | Local errors are always recorded and cleared on the next local success; a vanished local source drops its metrics; a complete local pass replaces the previous one; an unreadable log is an error | `a_local_failure_after_success_is_reported…`, `when_the_local_source_disappears…`, `a_local_pass_replaces…` |
| N05 | A future-dated local reading outranked every real one | Fixed | Observation times are capped at collection time | `a_future_dated_local_reading_cannot_outrank_real_ones` |
| N06 | Two local results in the same second were ordered by arrival | Fixed | Jobs carry an issue sequence; an older local job is ignored | `an_older_local_job_finishing_late_never_overwrites_a_newer_one` |
| N07 | Freshness was frozen at write time; a stale number could look healthy | Fixed | Snapshots carry `stale_after_secs`; the CLI and the UI re-judge freshness when they read. A stale reading gets no status colour and a dimmed pill | `freshness_is_judged_when_read_not_when_written` (CLI), `a_snapshot_read_later_is_rejudged…`, `refresh re-judges…`, `a stale reading is flagged…` (JS) |
| N08 | Resource bounds were not end to end | Mostly fixed | Bounded event queue (1,024, drops when full), 20,000 entries per folder, 100,000 records per log, future records pruned, totals summed without integer overflow | `far_future_records_are_dropped…`, `…huge_counts_do_not_wrap` |
| N09 | Detection and watcher setup ran under the engine lock | Fixed | Detection is a `Probe` run off the lock; the watcher has its own lock; the engine's own methods no longer touch the file system | `disabled_providers_are_not_probed` (asserts applying settings does no detection) |
| N10 | `Retry-After` was capped at an hour; Copilot fanned out over every stored login | Fixed, one gap | `Retry-After` honoured up to 24 h; Copilot de-duplicates, tries at most 3 logins, and stops on anything but an expired login | `rate_limit_backs_off_and_keeps_last_numbers` |
| N11 | A job that never returned left a provider "refreshing" forever | Fixed | 180-second lease; afterwards the provider can be polled again and the lost job's late answer is ignored. A failed thread start reports an error at once | `a_lost_online_job_does_not_block_polling_forever…` |
| N12 | The no-link rule covered descendants, not the walk root | **Accepted, documented** | See "Accepted" below | `junctions_are_not_followed` covers descendants |
| N13 | Malformed UTC offsets such as `+0:8` were accepted | Fixed | Only `+HH`, `+HHMM`, `+HH:MM` | `time::malformed_is_none` |
| N14 | "% used" on a money quota showed the remaining amount | Fixed | The amount shown matches the percentage shown | `describe: show-used on a money quota…` |
| N15 | The panel could be clipped on a short or high-DPI screen | Fixed in code, **not verified on such a screen** | Height is capped to the screen's available height; the list scrolls inside | None |
| N16 | Some tests could not fail for what they claimed; CLI tests could read real logins | Fixed for the cases named | CLI tests run with every default location redirected into the test folder and plant a fake login to prove it; detection is counted; read-only is checked by bytes and modification time; the renderer was exercised with hostile strings in the real WebView | `providers_lists_the_registry_from_a_sealed_environment`, others above |

### Baseline findings the reviewer marked partial or not fixed at `f0b3ea5`

| ID | Reviewer's remaining concern | Now |
|---|---|---|
| F03 | Private endpoints remain; partial success with missing reset data | Endpoints remain (risk R1). A missing reset time is reported as no reset time, which is what the provider said |
| F04, F11 | Unbounded walk and queue, no rotation identity | Addressed by N03 and N08 |
| F05 | Loose offset syntax | Addressed by N13 |
| F09, F13 | Local errors hidden, ordering | Addressed by N04, N05, N06, N01 |
| F10 | I/O under the lock, no job lease | Addressed by N09, N11 |
| F12 | Root links | Accepted, see below |
| F16 | Second writer, defaults on corruption | Addressed by N02. No `fsync`: a power cut during a save can still lose that save, and the backup then applies |
| F17 | README overstated security properties | README rewritten after the reviewed commit; claims now match the code |
| F18 | Desktop shell untested | Still true: the shell has no automated tests. Its behaviour was checked by hand through the WebView debugging port |

## Accepted, with reasons

- **The configured provider folder may itself be a link (N12).** People keep `~/.codex` or a `sessions` folder on another drive through a link, and refusing that would break working setups. The folder is one the user chose, so following it grants nothing new. Links *inside* it are never followed.
- **A file can be swapped between listing and opening (N12).** A local program able to do that already runs as the user.
- **`Retry-After` given as an HTTP date is not parsed (N10).** The request then backs off exponentially from 5 minutes to an hour.
- **`collect_once` has no single overall deadline (N10).** Each request has a 10-second timeout and an adapter makes at most three; the macOS Keychain call has no timeout of its own. The lease (N11) stops this from blocking later polls.
- **A log replaced with one that shares its first 64 bytes and is no shorter looks like an append (N03).** Token logs start with a timestamped line, so this needs a deliberate construction.
- **`Outcome` is `Option<Result<…>>`.** The reviewer would prefer distinct types for "no such source", "source gone" and "empty". The engine now treats a local `None` as "gone"; a richer type is deferred.
- **Upstream advisories with no fix available:** `glib` 0.18 (unsound iterator, Linux GTK stack, pulled in by Tauri) and `proc-macro-error` (unmaintained, build-time). Both are warnings in `cargo audit`, not vulnerabilities.

## Checks run

| Check | Result | When |
|---|---|---|
| `cargo audit` | 0 vulnerabilities, 2 warnings (above). Before updating: 7 vulnerabilities in the lockfile inherited from 0.2.1 (rustls, rustls-webpki, webpki-roots, bundled SQLite) | 2026-11-16 |
| `npm audit` | 0 vulnerabilities. Before: 2 high in the Vite dev server | 2026-11-15 |
| Secret scan | gitleaks over the full history passes in CI, with an allowlist (`.gitleaks.toml`) naming nine fake credentials that the tests plant on purpose. A separate `git` search of every added line in history found only those fixtures and a lockfile hash | 2026-11-16 |
| Hostile strings through the real renderer | Markup and script in every provider-supplied field: no element created, no script run, text shown literally. Windows, WebView2 157 | 2026-11-16 |
| Tokens in output | Fake tokens planted in fixture login files never appear in CLI output or errors | Automated |
| Read-only CLI | Data folder byte- and time-identical after `--live` | Automated |
| Capabilities | Webview has `core:event:default`, window drag, window close. No shell, file-system or HTTP permission | By inspection |

## Not done

- No penetration test, fuzzing or third-party audit.
- The desktop shell (threads, publication, window handling) has no automated tests.
- Nothing was tested on macOS: the Keychain path, in particular, is unverified.
- No test against live provider endpoints.
- Installers are unsigned.
