# Fresh-context Codex challenge review

2026-11-16. Original code: `fe4c1ef1d46d4701cb38ac5d882931dde6110053`.
Reviewed candidate: `dfdcfe697ff2d5765ffd11aff5a26202b25e2227`.
This review was performed by a separately dispatched Codex context, without Claude,
paid external tools, real provider credentials, live provider requests, or GUI input.
The original reports `0003`, `0004`, and `0005-codex-release-hardening.md` were read as
allegations. Their dispositions and test totals were not accepted as proof.

**Candidate decision: FAIL; release-blocking YES.** Five residual defects below violate
the requested incomplete-data or bounded-wait/provider-minimum criteria. None establishes
a critical/high exploit. Repairs made by the coordinator after these findings are outside
this candidate decision and require a separately identified recheck.

All source references below refer to the candidate, even if the shared worktree later changes.

## Findings

| ID | Severity / blocker | Evidence and independent reproduction | Required correction |
|---|---|---|---|
| C01 | Medium / YES | **Text CLI hides incomplete coverage.** `crates/quotis-cli/src/main.rs:173` formats used values without `Metric.qualification`; `:221` advice can call a partial result connected and reporting. Executed actual candidate CLI with an isolated Gemini folder containing only `unknown telemetry\n`, all other providers disabled. `status --live --offline` prints `Gemini CLI connected 5h 0 tokens used · 24h 0 tokens used`. `diagnose --provider gemini --live --offline` calls both metrics `Fresh` and advises `Connected and reporting.` The corresponding JSON correctly contains `qualification: partial: not every log could be read in full, so this is a lower bound`. A consumer using the text interface receives an unqualified false zero. | Display qualification in status and diagnose text and qualify advice. Test the real CLI output with incomplete data, including cached snapshots. |
| C02 | Medium / YES | **The unknown-schema repair trusts a type label too broadly.** `crates/quotis-core/src/providers/tokenlog.rs:179` whitelists every object whose type is user/system/info/error/tool. Executed the actual registered Gemini adapter against `{"type":"info","usage":{"total_tokens":900}}\n`: `used=Some(0.0), qualification=None`. Removing only the type field correctly produces a partial qualification. Thus a usage-schema change wrapped in a familiar record type still certifies zero. This is a synthetic changed schema, not a claim about a currently emitted vendor format. | Recognize known non-usage shapes conservatively; unfamiliar fields that may contain telemetry must retain incomplete coverage. Add a known-type/unknown-fields regression. |
| C03 | Medium / YES | **The live diagnose command escapes the shared deadline.** `crates/quotis-cli/src/main.rs:108` invokes bounded collection, but diagnose subsequently constructs another engine and calls synchronous `probe_now()` at `:329`/`:330`. That probe runs every enabled provider inline (`crates/quotis-core/src/engine.rs:165`), including filesystem/database/credential discovery, after the live wait budget has been spent. The providers command likewise probes inline at CLI `:276`. Independent synthetic provider probe: `collect_once_with_timeout(false, 10 ms)` returns with an error in <100 ms while detection sleeps 150 ms; calling the same public `probe_now()` afterward takes >=150 ms. This proves the bypass primitive; a real OS/filesystem hang and an end-to-end native CLI hang were not induced. Cached diagnose also performs discovery beyond the cache read. | Reuse detection/status evidence from the bounded collection. Keep cached diagnosis read-only without an extra unbounded discovery pass, or include all command work in one explicit deadline. |
| C04 | Medium / YES | **Retry-After is anchored before the response.** `crates/quotis-core/src/engine.rs:327` records `last_poll` at job issuance; the rate-limit path at `:411` stores response-relative delay, while `all_online` at `:314`, manual refresh and due checks compare against issuance time. Independent public-engine reproduction: issue one online job; wait 1,100 ms to represent the response time; finish it with `RateLimited` and `retry_after_secs=Some(1)`; immediately call `all_online()`. Observed: one replacement issued at 0 ms after the response, despite its one-second not-before instruction. HTTP-date parsing similarly calculates a response-time delay in `net.rs:33`, then the engine shortens it by the preceding request duration. Numeric cap removal/date parsing alone do not close N10. | Track a response-time retry anchor or explicit not-before instant and make all scheduling entry points honor it. Keep ordinary polling cadence separate. Test a delayed response, not only a large stored integer. |
| C05 | Medium / YES | **Worker exhaustion can silently become not detected after reconfiguration.** `crates/quotis-core/src/engine.rs:180` preserves pending work while resetting errors; `:241` silently omits new local work at the cap. `collect_once_with_timeout` at `:490` only marks newly issued, unfinished work unavailable. Independent public-engine probe retained three local jobs through four directory revisions without reporting completion, then requested collection on the latest revision. Observed `held jobs=3; replacement collection: status=NotDetected, error=None, refreshing=false`. No actual hung threads were created: unreported issued jobs model their engine state. If those jobs are permanently stuck, the new source is never checked and the snapshot does not explain why. | Keep worker caps, but expose unavailable/pending coverage when exhausted slots prevent checking the current configuration. Add a reconfigure-at-cap snapshot assertion, not only an issued-job count. |

## Challenged repair dispositions

| Scenario / earlier allegation | Candidate disposition and evidence |
|---|---|
| Backup succeeds, primary fails; failed enable followed by corruption; N02 | **VERIFIED FIXED within the tested transaction model.** `settings.rs:124` chooses the authoritative committed record. Preparation at `:164` cannot select a revision; commit replacement at `:167` does. Independent external probe saved Codex disabled, replaced primary with a directory, attempted default/enabling save, required an error, loaded, replaced primary with corrupt bytes, and loaded again: disabled settings survived both reads. The maintained injected commit/rollback-failure test also passed. |
| Corrupted settings and provider defaults | **VERIFIED FAIL-CLOSED for unreadable/unparseable existing records.** Independent probe corrupted the committed record while compatibility files remained and asserted every registered provider disabled. `settings.rs:138` constructs explicit disabled entries; the maintained both-legacy-files-corrupt test passed. Genuine absent first run still uses intended defaults. Valid but manually edited partial JSON is accepted settings, not a transactional corruption detector; no checksum/authenticity claim is made. |
| Drag during save, immediate normal exit; S01 | **VERIFIED at extracted production coordinator; native event path UNVERIFIED.** `src-tauri/src/persistence.rs:16` writes before runtime mutation, `:20` preserves a position recorded during disk I/O, and `:29` flushes the current position under the same save lock. The actual module's movement/write/flush test passed. Normal quit calls flush at `src-tauri/src/main.rs:355`; ExitRequested calls it at `:433` and prevents exit on failure. No real window drag, DPI, monitor, event-delivery ordering, forced-termination or power-loss guarantee was tested. |
| Overlapping settings saves; R03/R04 | **VERIFIED at production coordinator.** The actual module's failed-write test leaves runtime unchanged and never publishes. Its controlled stalled-publication test starts a second save and checks ordered publications plus disk/runtime equality. `persistence.rs:13` retains SAVING through capture, write, runtime update and publish. These tests execute the extracted coordinator, not a native Tauri event transport. |
| Repeated reconfiguration with outstanding local/online work; R05 | **VERIFIED bounded issuance for one retained Engine; reporting PARTIAL.** `engine.rs:180` carries both pending-ID sets across generations; `:237` caps local and `:210` caps online at three each/provider. Independent public API probe changed directories 20 times without completing any work: exactly three local plus three online jobs were issued. Shell configuration discovery now has one worker and one queued request (`main.rs:414`), so reconfiguration does not spawn unlimited probe threads. Permanently stuck OS work is bounded, not cancelled; one hung configuration pass can delay later discovery. C05 exposes the missing current-configuration unavailable state at the cap. |
| Duplicate old reports | **VERIFIED FIXED.** Independent probe repeatedly delivered one old local and online completion ten times after reconfiguration; exactly one slot per kind became available. Pending IDs are removed once before generation rejection (`engine.rs:344`); duplicate messages no longer free unrelated slots. Maintained duplicate test also passed. |
| Persistent local-error provenance; N04 | **VERIFIED FIXED for the named failure sequence.** Separate local/online slots survive online error then success, and the maintained PermissionDenied -> Offline -> success regression passed. Snapshot prioritizes local failure (`engine.rs:445`); this deliberately exposes one error at a time, not both. |
| Unknown telemetry and false zeros; N04 | **PARTIAL / FAIL.** Plain malformed/unknown lines now remain qualified across passes; maintained tests passed. C01 strips that qualification from text presentation, and C02 bypasses qualification in core for a known type carrying unknown fields. |
| Partway listing failures and global walk cap; S02/N08 | **VERIFIED FIXED for enumerated mechanisms.** `fsx.rs:74` counts iterator results, including errors, against the shared budget and sets incomplete instead of flattening them away. The maintained injected error-between-two-real-entries test and empty-tree global-budget test passed. `:35` propagates directory errors, file limits, depth and exhausted budget. This bounds enumeration count, not latency of an individual OS call or total token-log memory; the stated per-file record/file caps remain large. |
| Shared-header rewrite / replacement identity; N03 | **VERIFIED FIXED for ordinary tested replacements.** Maintained longer shared-header rewrite and new file identity with preserved size/mtime/header/tail tests both passed. `fsx.rs:132` holds identity and checks consumed boundary bytes. An in-place rewrite that preserves identity, timestamp and sampled regions while changing an unsampled middle remains an acknowledged limitation, not a verified complete-file integrity guarantee. |
| One-shot local/detection deadline; N10 | **Core primitive VERIFIED; CLI end-to-end PARTIAL / FAIL.** The maintained 300 ms collector/30 ms wait test passed, and independent 150 ms detection/10 ms wait probe returned promptly with Unavailable. Outstanding work keeps its slots. C03 shows a subsequent CLI path outside the primitive. |
| HTTP dates and very long Retry-After; N10 | **Representation repair VERIFIED; scheduling PARTIAL / FAIL.** Loopback HTTP-date and `u64::MAX` backoff tests passed; parsing rounds up fractional seconds and saturating scheduling avoids overflow. C04 establishes the remaining not-before violation. |
| Freshness / future observations; R01/N05/N07 | **Named existing regressions passed.** Repeated future local record is rejected without restamping, stale/expired cache values are rejudged at read/display time. No live vendor clock-skew or native periodic rendering certification. |
| Publication delivery; N01 | **PARTIAL remains.** Snapshot cache pending state retries failed writes. `src-tauri/src/main.rs:123` still discards `app.emit` failure before advancing published body. This is a source-only delivery-retry gap; no native emission failure was induced. It is separate from ordered settings publication. |
| Assurance and credential boundaries; N16 | **Local executable evidence only.** Tests exercise synthetic sources and loopback HTTP. No actual secret/keychain material was read for this review. This does not certify secret redaction for every future upstream field or prove native safety from formatting-only JS tests. |

## Executed evidence and boundaries

Independently ran `scripts/with-fixtures.ps1 { cargo test --workspace --locked --offline }`
against the candidate before coordinator changes: 77 core, 30 engine, 13 CLI, and four shell
tests passed. Final isolated JavaScript run through the same wrapper: 17 passed. Their
passing totals do not override C01-C05. The Rust suite was inspected for actual assertions,
including settings failure injection, controlled publication order, pending IDs, filesystem
iterator errors, preserved file metadata, and the deliberately short deadline.

External synthetic probes are preserved outside the repository in
`C:/Users/User 1/AppData/Local/Temp/quotis-challenge-13620d2b-ad9b-451e-9a61-dc1ebc07d0d6/`:
`challenge.rs`/`challenge.exe` cover settings, detection deadline, worker accounting and
known-type telemetry; `retry.rs`/`retry.exe` cover response-time retry scheduling;
`exhausted.rs`/`exhausted.exe` cover reporting when old local jobs consume all slots. They were
compiled directly against existing debug rlibs, with no dependency download or implementation
edits by this reviewer. C01-C04 used candidate artifacts. The later C05 probe used the shared
rlib after the coordinator's retry-anchor changes; the reconfiguration, local cap and deadline
branches involved in C05 were compared with the candidate and remained unchanged. Each was
run through the fixture wrapper. The actual CLI fixture
used a temporary `settings.json`, explicit synthetic Gemini path, all other providers off,
and malformed telemetry only. JSON/text observations above came from that subprocess.

The wrapper redirects HOME, USERPROFILE, APPDATA, LOCALAPPDATA, XDG_CONFIG_HOME,
CLAUDE_CONFIG_DIR, CODEX_HOME and QUOTIS_DATA_DIR, removes OPENROUTER_API_KEY and
DEEPSEEK_API_KEY, and preserves Rust toolchain/cache roots. No attempt was made to cross
the earlier denied real-credential boundary. No external paid tool or Claude was invoked.

| Release criterion | Candidate decision |
|---|---|
| No silently committed failed provider changes | PASS for tested single-process settings transactions and conservative recovery |
| Bounded workers through reconfiguration | PASS for tested retained-Engine accounting and bounded shell configuration worker |
| Honest incomplete/freshness states | FAIL: C01, C02 and C05 |
| Read-only CLI with bounded live wait | Read-only fixture checks PASS; total live wait FAIL: C03 |
| Respect provider retry minimum | FAIL: C04 |
| No secrets/credential access or arbitrary GUI input during review | PASS for this review's actions |
| Native Windows drag/quit, real provider operation, macOS/Linux behavior | UNVERIFIED by this reviewer |

Release-blocking remains **YES** for this exact candidate. Any later repaired SHA needs
the five challenge scenarios rerun before changing this decision; unavailable native,
cross-platform and live-provider checks must remain UNVERIFIED rather than becoming PASS.

## Narrow recheck: bf742004a418530435e14c1e31f4a641500d8aae

Compared the exact repair commit against `dfdcfe6`, inspected the changed production paths
and assertions, and independently reran 32 engine tests, 11 token-log tests and 14 CLI tests
through `with-fixtures.ps1`. All passed. The original settings/worker/duplicate external
probe was recompiled against this core and still passed its safety assertions.

| Finding | Recheck result |
|---|---|
| C01 | **VERIFIED FIXED.** CLI `main.rs:190` emits each qualification and `:253` qualifies advice. Repeated the actual unknown-telemetry subprocess fixture: status and diagnose now print both partial lower-bound warnings, and advice explicitly says the total is incomplete. A synthetic cached snapshot also retains the caveat. |
| C02 | **VERIFIED FIXED for the named schema variant.** `tokenlog.rs:196` requires known non-usage fields as well as a known type. The external registered-adapter probe now returns partial qualification for `type=info` plus `usage.total_tokens`; the unknown-type control remains qualified. Nested arbitrary content is not a claim of universal schema recognition. |
| C03 | **Diagnose mechanism VERIFIED FIXED; overall finding PARTIAL at this SHA.** Live load retains `engine.statuses()` at CLI `:111`; diagnose reuses it at `:339`. Cached load constructs pure statuses without probing at `:121`, and actual cached JSON reports `found_locally=null` / `detection_source=not_checked_cache_only`. There is no second diagnose discovery. However `providers --live` is still accepted and reaches synchronous `probe_now()` at `:284`; this accepted live invocation remains outside the wait bound. The coordinator proposed rejecting that flag before discovery and documenting standalone providers as explicit synchronous local discovery. That would close the stated bounded-live criterion, but it is not in this SHA. |
| C04 | **VERIFIED FIXED.** `engine.rs:418` starts backoff when an accepted response arrives; `:214` gates every online scheduling entry through that elapsed backoff. Independent wall-clock probe issued a job, waited 1,100 ms, finished Retry-After=1 second, and observed zero immediate replacements; after another 1,050 ms, exactly one was available. Maintained tests also cover due/manual/all-online scheduling and a delay beyond a week. |
| C05 | **VERIFIED FIXED.** Local exhaustion now reports Unavailable at `engine.rs:250`; online exhaustion after generation changes does likewise at `:227`. External three-held-local-jobs/four-revisions probe now returns Error with `previous local checks have not finished; worker limit reached`. Reporting the old jobs releases their slots once; a subsequent current-configuration collection clears the error. Old-generation results do not themselves clear the current failure or mutate current observations. |

The new aggregate token retention limit was also inspected: `tokenlog.rs:138` shares a
200,000-record remaining budget through the file pass, truncates/shrinks retained vectors,
and preserves partial coverage when it discards entries. The actual three-file/300,000-pair
regression passed. This caps retained timestamp/count pairs at approximately 3.2 MB, not
all process memory: file state, handles, parse buffers, and transient append allocations
remain additional. No process-memory or cross-platform file-descriptor guarantee follows
from that one assertion.

The recompiled external recheck probes are `retry_recheck.rs` and `exhausted_recheck.rs`
in the previously recorded temporary folder. Source/CLI observations in this section refer
to `bf74200`; the coordinator's subsequent working-tree CLI flag change was excluded from
the exact-SHA disposition.

**Decision for bf74200: release-blocking YES for the remaining C03 providers/live path.**
All unavailable native, cross-platform and live-provider checks remain UNVERIFIED.

The optional free-skill folder's local validator also passed (10 text files, 42.5 KB), but
that is structure/sample consistency, not semantic proof. A separate substantive issue
was reported to the coordinator: `commercial/agensi/references/planning-playbook.md` divides
raw `used` by elapsed time while treating `left` and thresholds as percentages. Money or
request quotas therefore need normalization before projection; a $7.50-used/$20-total
quota is 37.5% used, not 7.5%. The listed inputs also permit remaining-only data while the
formula requires used. Stale-input projected exhaustion times need an explicit observation
time rather than presentation as a current countdown. These are packaging-content concerns,
not additional code findings. No platform packaging/policy assertions were independently
verified online by this reviewer.

## Final source recheck: 57adf80d2b5ceb8691361f1e5fdbe64bf1d4236c

Inspected the exact `bf74200..57adf80` code/test diff: it only adds the providers/live
argument rejection, clarifies the live help text, and includes that invocation in the
existing bad-usage regression. Independently ran that maintained test through the fixture
wrapper: PASS. Actual isolated subprocess checks for `providers --live`,
`providers --live --offline`, and `providers --live --json` all returned usage exit 2 before
discovery. The JSON variant returned the structured usage error.

**C03: VERIFIED FIXED for the stated bounded-live scope.**
`crates/quotis-cli/src/main.rs:88` rejects providers/live during argument parsing; the
other live commands retain the bounded collection path, and diagnose reuses its detection
evidence. Standalone `providers` remains explicit synchronous local discovery. It can wait
on blocking OS/filesystem/database discovery, so this review does not certify a deadline
for every CLI command, settings-file read, output write, or underlying OS operation. The
60-second promise concerns waiting for live collectors, as the revised help says.

**Final source challenge judgement for 57adf80: PASS within the reviewed criteria;
release-blocking source finding NO.** All five challenge findings have verified repairs.
This carries forward the executable evidence above without representing the narrow final
recheck as another complete suite or native-runtime run. The source-only event-emission
retry gap recorded under N01 remains a limited delivery concern, with no induced native
failure and no broader reliability certification.

**Public release readiness remains blocked/unverified outside this source judgement.**
Native drag/quit event integration, real provider agreement, macOS/Linux execution,
installation/uninstallation, signing and clean-machine packaging are not certified here.
An application executable build does not prove that an installer succeeds; the coordinator's
reported isolated NSIS bundler panic was not independently investigated by this reviewer.
Unavailable tests remain UNVERIFIED. No release, installer or marketplace approval is
implied by closing C01-C05.

The updated ignored free-skill playbook was also inspected after the optional arithmetic
report: it now normalizes both percentages, requires a meaningful total for money/request
quotas, declines incomplete inputs, and anchors historical exhaustion to the observation.
Those text corrections resolve the specific projection concerns in the revision read.
The offline validator's PASS remains a local packaging/consistency result rather than
evidence of an Agensi upload, delivery test, marketplace approval, or broad agent reliability.

No implementation files were edited by this reviewer, no commits or pushes were made by
this reviewer, and no reviewer-owned background work remains.

## Overlay follow-up: 72e400c5cdf97fd967ff66f7fcb10a112d808634

Inspected the exact two-file change from `57adf80`. Independently ran all 18 JavaScript
tests and the production web build through the fixture wrapper: PASS. No Rust source
changed, so no additional Rust suite was needed for this scoped recheck.

**Named partial-local-zero overlay repair: VERIFIED FIXED.** `src/format.js:80` renders
partial local values as lower bounds (`≥0`) with a visible `partial lower bound` detail.
The collapsed pill uses that value and now includes the detail in its title/accessibility
label through `:140`. Partial quota metrics get unknown level at `:40`, incomplete text at
`:69`, and are excluded individually from `tightest` at `:86`. Existing freshness, money
display, pinning, warning, error and hostile-text formatter tests still pass. The unchanged
DOM code continues to insert these strings as text. This is formatter/source evidence;
native WebView rendering, clipping, tooltips and accessibility behavior remain UNVERIFIED.

Two limited caveats were reproduced with direct synthetic calls to the actual formatter:

- **Mixed complete/partial quota case, future-facing integrity concern:** a provider with
  complete 5h=90% remaining and partial week=5% returns green/ok 90% with no pill caveat,
  because the uncertain quota is dropped before provider ranking. Thus the claim that
  partial quotas cannot produce green/ranked providers holds for the individual partial
  metric and an all-partial provider, not every mixed provider. Current adapters only emit
  the `partial:` prefix on local usage, so this fixture has no current production quota
  producer; it is not a new release blocker for the supplied adapters. Any future partial
  quota producer needs provider-level uncertainty handling and a mixed-window test.
- **Low, nonblocking tooltip regression:** when cached local metrics coexist with a
  provider error, the new fallback detail can replace the previous specific error message
  in the pill title. A partial local metric plus PermissionDenied now yields
  `fake: No access 5h in 5h · partial lower bound` rather than including
  `permission denied reading local data`. The pill still says No access and the expanded
  card retains the full error. Preserve both error and metric caveat in a later tooltip
  refinement; this does not lose the backend error or silently claim success.

**Scoped source judgement for 72e400c: PASS for the named overlay repair, with the two
qualified caveats above; no additional release-blocking current-source finding.** Public
release/platform/provider/installer limits from the preceding decision still apply.
No reviewer-owned background work remains.

## Movement-event follow-up: a99290e37365b09df1435712a5d6afa7d5df91d9

Reviewed the exact `059ff3f..a99290e` diff and the surrounding layout, move callback,
settings flush and exit paths. Independently ran the actual shell's six persistence/movement
tests through `with-fixtures.ps1`: PASS. The preceding `059ff3f` fixture-name fix was also
inspected: adding an atomic sequence to PID/time makes concurrently created test directories
distinct without changing production persistence.

**Timing-grace mechanism: VERIFIED FIXED at the production state helper.**
`src-tauri/src/main.rs:60` replaces the 400 ms veto with an explicit last-placement
destination. Any non-notch move differing from that destination is recorded immediately;
clearing the marker permits dragging back to the earlier destination. The actual helper
tests verify immediate move -> normal flush -> reload and app-placement -> user move ->
drag back. These are state/real-disk tests rather than simulated native mouse input.

**Lock review: no new deadlock cycle established.** Layout records the marker under App,
releases App, then calls native placement/size APIs (`main.rs:235`). The move callback reads
native position and width before acquiring App (`:245`); `remember_move` then performs
only in-memory changes. There is no new App-held native-window call or reverse nested
window/App locking edge in this diff. The existing save coordinator still serializes disk
and publication, and exit flush uses that same order.

The current-position check rejects queued events whose positions are already obsolete.
It does not make the native position check, width read, later App lock, or quit event
atomic; actual platform delivery ordering, resize/DPI behavior and a final drag/close run
remain UNVERIFIED by this reviewer. The coordinator reported that the older production
build's targeted move/close probe preserved a null position; that observation is evidence
against the older timing-grace path, not proof that the new executable's native path passed.

**Low, source-only native-error edge:** layout sets `placed_position` before calling
`set_position`, then discards the native call's error. If placement fails without a movement
event, the intended destination marker can remain indefinitely. A later user move that
lands exactly at that destination would then be mistaken for the app's placement and
ignored. Clear or reconcile the marker on placement failure in a future hardening pass.
No such native error was induced here; this is not an established release blocker.

**Scoped source judgement for a99290e: PASS for the named movement-state repair; no new
release-blocking source defect or deadlock established.** Native execution of the repaired
move/close path is still a separate gate. Earlier installer/platform/provider certification
limits and the limited nonblocking source concerns remain. No reviewer-owned background
work remains.
