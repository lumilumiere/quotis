# 0006: native Windows distribution follow-up

Baseline `2938e21459d3fdca0711c7e7f456cbdb9320318d`; final repair source
`4718a9b545379d1dfd2799db2d56323bb1d354aa`. Codex only. No architecture rewrite.
The earlier v0.2.1 audit was not repeated. **Public release remains FAIL** because mandatory
native installation, hardware, account and distribution gates remain UNVERIFIED.

| Finding | Severity / old evidence | Remedy and verification | Disposition |
|---|---|---|---|
| W01: Settings discovery badge never follows collection | Medium. At2938e21 `src/settings.js:61`, badge uses only p.found, with no usage subscription. In an isolated production Windows process, enable synthetic Gemini after opening Settings: cache/overlay collect 42 but Settings stays Not found until reopened. | `5df6cb0` subscribes before initial reads and uses current collection/error evidence. Extracted old badge expression gave1 passing/3 failing helper checks; repaired checks passed. Native Connected/Issue/recovery transitions verified without reopening. Current code `src/settings.js:58`, `:139`; helper `src/connection.js:1`. | Fixed; required for truthful connection presentation |
| B01: event/RPC capture race hides a newer error | Medium. At5df6cb0 `src/settings.js:131`/`:135`, any earlier event prevents newer initial get_usage from applying. A separate context executed actual settings.js with injected IPC/minimal DOM. Both equal- and different-second snapshots reproduce stale Connected instead of Issue. A delayed older event can also overwrite an initialized error. | `1a0dc36` adds desktop-only capture revisions, assigned with snapshot under App mutex (`src-tauri/src/main.rs:68`, `:145`, `:329`). Both renderers reject older revisions. Core/cache/export format unchanged; getter still avoids PUBLISH/disk waits. Actual-entry regressions cover both transport directions, delayed events and recovery. | Independently rechecked fixed at1a0dc36 |
| B02: discovered but uncollected provider called Connected | Medium. At5df6cb0 `src/connection.js:4`; actual Engine.snapshot at `crates/quotis-core/src/engine.rs:462` emits Connected for detected-only state with no observed_at/data. Public-core fixture and actual-settings fixture reproduced this. | `1a0dc36` requires successful collection observed_at; discovery-only becomes Checking. Notes-only/unlimited successful results remain Connected. Tests use actual snapshot shape. | Independently rechecked fixed at1a0dc36 |
| B03: concurrent save responses restore obsolete enablement in Settings | Medium. At1a0dc36 `src/settings.js:111`, independent change callbacks invoke concurrent saves and accept every returned provider list. Deliver valid usage in order, then deliver B's status reply before A: final badge becomes Off although last committed state is enabled; inverse becomes Checking. Backend write mutex does not order frontend transport replies. | `1ee544e` snapshots each request and serializes invoke plus response (`src/settings.js:108`). Old actual-entry rapid-save regression fails because two writes issue together; repair permits one until prior response, then preserves both requested configs. First/last failed-save cases do not poison queue or assert a failed disable. | Independently rechecked fixed at1ee544e |
| W02: explicit Settings close destroys pending queue | Low introduced lifecycle concern. At1ee544e `src/settings.js:132`, Close/Escape destroy window immediately while later saves may remain uninvoked. Actual-entry callbacks close before first pending write resolves: two regression failures. | `4718a9b` awaits queue on explicit Close/Escape (`src/settings.js:132`). Both regressions pass. Native normal Settings close/quit remains checked separately. | Explicit controls fixed; forced OS/WebView/whole-app termination during unacknowledged drafts remains UNVERIFIED |

Red/green evidence was produced before each repair:

- B01/B02 targeted helper plus actual-entry suite: 11 tests, 7 passed/4 failed before repair;
  11 passed afterward. Failed cases were two discovery assertions and two stale-error cases.
- B03 actual-entry suite:7 tests, 6 passed/1 failed before serialization;7 passed afterward.
- W02 actual-entry suite:9 tests, 7 passed/2 failed before awaiting saves;9 passed afterward.
- Final complete frontend suite:33 passed, 0 failed/skipped. Shell capture/cache contract
  plus existing shell suite:7 passed. Full Windows Rust suite:131 tests (78 core, 32 engine,
  14 CLI, 7 shell), zero failed/ignored. Cross-platform CI totals are in the readiness report.

[Fresh-context challenge record](0005-fresh-context-challenge.md) preserves the separate
reviewer's original failures, adverse interleavings, exact-commit rechecks and limitations.
This was another Codex context, not an independent model family. Native fixture interaction
does not independently prove every transport interleaving; maintained actual-entry tests
provide that evidence. Stubbed DOM tests do not certify rendering/accessibility appearance.

No new unsafe HTML sink or credential access was found: provider text still uses textContent
or ordinary text attributes. On-disk cache schema remains 1; revision is desktop IPC metadata
only. Known security limits and two upstream audit warnings remain documented.

Native test evidence, installer signatures/provenance, resource measurements, protected
owner installation and packaging checks are in [distribution validation](../DISTRIBUTION_VALIDATION.md).
Read-only MSI extraction is not installation certification. All real providers remain
UNVERIFIED and the FREE skill still requires owner/account/legal publication decisions.
