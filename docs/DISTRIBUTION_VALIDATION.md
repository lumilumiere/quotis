# Final distribution validation, 2026-11-16

Source: `4718a9b545379d1dfd2799db2d56323bb1d354aa`, development branch `quotis-2.0`.
Continued from `2938e21459d3fdca0711c7e7f456cbdb9320318d`. **Public desktop release: FAIL.**
See [current decisions](RELEASE_READINESS_FINAL.md), [machine evidence](validation/windows-distribution-final.json)
and [native test specification](NATIVE_RELEASE_CHECKS.md). UNVERIFIED is not PASS.

## Windows evidence and isolation

Native host: Windows 11 Pro x64, 10.0.26300.9818, DisplayVersion 26H2; Intel i5-1135G7,
8 logical processors, approximately 15.7 GiB visible RAM. This one host does not establish
compatibility with stable Windows releases or other hardware. One display was available;
no monitor, DPI or system-wide network settings were changed.

The existing 0.2.1 installation was never launched, replaced, upgraded or uninstalled.
Its version and executable SHA256 were compared before/after; registration remained
present, with no Quotis autostart entry in either Run hive. Native apps ran copied
executables in unique repository target folders with isolated homes/config/provider
directories and WebView profiles. Only a synthetic Gemini source was enabled; API-key
variables were removed. Autostart was never toggled. This is process/profile isolation,
not a disposable operating system or installation certification.

Orca's Windows UI Automation became available. Controls were inspected and operated
through Invoke, Toggle and ExpandCollapse patterns, restricted to the recorded child
PID and owned HWND. No global keyboard/mouse injection or focus bypass was used.
The foreground application occluded window-region captures; those captures were rejected
as Quotis visual evidence and are not included. Theme/mode ComboBox SetValue returned
without changing state; this attempted interaction is **UNVERIFIED**, not successful.

The retained final native reports enumerate each passing assertion and timestamp:

- Empty overlay, expand/collapse/reopen, Settings open/close and normal Quit.
- Provider enablement and rapid repeated toggles; final disk/runtime/control agreement.
- 42-token synthetic collection and live Connected badge without reopening Settings.
- Glass, hover expansion and topmost controls persisted.
- Unknown log input reached the overlay as `≥42` and a partial lower bound.
- Exclusive local-file lock retained 42-token metrics with an error; Settings showed
  Issue, then recovered to Connected when the source recovered.
- Settings primary/backup/committed replicas agreed after normal quit.

All three overlay modes also underwent startup, snapshot, normal shutdown and mode
persistence checks; scoped Island/Panel moves persisted on immediate normal close.
These prove native behavior/data transitions, not appearance or manual dragging.
Restart and read-only CLI checks are separately identified in the machine evidence.
No fixture result is represented as a real-provider comparison.

Earlier harness failures are retained locally. Two checks searched a stale computed
checkbox name even though its separate accessible Text node correctly showed Issue.
The test now reads the live badge Text node. Another check did not retain the Process
handle needed to read ExitCode after exit; it now retains the handle before Quit.
These failed attempts were not counted as passes. Actual-entry JS regressions separately
reproduced application defects; they were not inferred from these harness failures.

## Engineering repairs discovered during certification

[review 0006](reviews/0006-windows-distribution-validation.md) records severity, old/new
file references, reproductions and red/green results. The native stale discovery badge
was repaired in `5df6cb0`. A separate Codex context then challenged that repair and
reproduced two missed cases: discovery-only Connected and older events hiding newer
errors. `1a0dc36` adds successful-collection provenance and process-local desktop
capture revisions under the App mutex. Cache/export schema remains unchanged; the
in-memory getter does not wait for cache filesystem writes.

The same reviewer reproduced reordered concurrent-save replies. `1ee544e` serializes
immutable frontend save requests/responses; failure does not poison the next operation.
`4718a9b` makes explicit Settings Close/Escape await pending saves. Separate Codex
rechecks cover B01/B02/B03 at their exact repair commits in the
[challenge record](reviews/0005-fresh-context-challenge.md). This is another Codex
context, not an independent implementation/model-family review.

Forced app/WebView termination, OS shutdown and whole-app quit while Settings drafts
remain unacknowledged are not certified to drain those drafts. Previously acknowledged
settings remain committed. Slow-I/O whole-app lifecycle testing belongs in the VM gate.

## Installer provenance and outstanding certification

The final executable, NSIS and MSI were rebuilt locally from the identified source using
the locked dependency graph. [Artifact hashes](WINDOWS_ARTIFACTS_SHA256SUMS.txt) and
Authenticode statuses are retained. All four artifacts, including the separate CLI, are
**NotSigned**. No installer was uploaded or publicly released.

Read-only MSI inspection records ProductVersion 2.0.0, ProductCode
`{D765BDDC-1003-4724-A5A2-04A3D843F422}`, UpgradeCode
`{38F8D64F-ECE7-5519-BC6D-8603B3724DB4}`. MSI ALLUSERS=1 requests machine-wide
installation; NSIS uses currentUser. Manufacturer/publisher defaults to `github` and
needs an owner identity decision. Both desktop packages contain only the GUI executable;
the optional CLI is a separate artifact and is not installed onto PATH.

MSI administrative extraction runs only after an allowlist check excludes custom actions
from AdminExecuteSequence. It extracts files into a new target folder without installing
or registering the application. The extracted executable is tested separately. Tauri
patches its bundle-type marker, so payload/standalone hashes can differ; the evidence
records byte differences rather than asserting identical hashes. This is payload
provenance, **not** successful installation/upgrade/uninstall certification.

Sandbox is installed/enabled, but C: had less than 0.5 GB free versus Microsoft's 1 GB
minimum. It was not launched; no owner files were reclaimed. A later optional-feature
WMI query itself returned “not enough space on disk”; that preflight error is retained.
[Microsoft requirements](https://learn.microsoft.com/en-us/windows/security/application-security/application-isolation/windows-sandbox/windows-sandbox-install).
NSIS/MSI installation, old-version upgrade, uninstall, registration cleanup and
clean-machine WebView2 provisioning remain **UNVERIFIED**. Default bootstrapper behavior
may contact Microsoft if WebView2 is absent; offline installation has not been certified.

## Provider and FREE skill evidence

No named real provider account was authorized; prior credential denials were preserved.
All nine adapters remain fixture-tested/live-UNVERIFIED. There are no actual provider
screen readings, matching account timestamps or real reset-window comparisons to report.
[Provider matrix](PROVIDER_MATRIX.md) supplies the exact authorized comparison procedure.

The separate ignored commercial workspace is present. Local branch
`codex-free-distribution-validation`, commit `2e2ae19b08668ca3c214447b29399d03b47fb023`;
not pushed. Candidate ZIP `commercial/dist/quotis-ai-usage-intelligence-1.0.0.zip` is
20,415 bytes, 10 text files, SHA256
`CD4C89BABF8C748B9C8A6911E75848B960DE05DCC79FC9419896F9DD98F252C7`.
Every archive entry matches its source, with no executables, traversal paths or bundled
dependencies. Previous ZIPs remain locally preserved. Structure/content checks and four
offline arithmetic/instruction scenario groups pass. No Claude scenarios were rerun.

Copy now discloses GUI-only installers, separate unreleased CLI dependency, approved
optional live network/credential access, stop-on-denial, historical samples and lack of
live-provider verification. Incorrect fixed refresh/rounding assumptions were removed.
Offline setup uses a compatible supplied export or the bundled historical synthetic demo.

Technical package preparation is PASS. Publication remains **UNVERIFIED**: LICENSE.md
still says DRAFT, the privacy draft has owner identity/contact/date placeholders, and no
creator session/profile or marketplace delivery was authorized/tested. Do not upload until
those decisions are complete; rebuild/revalidate after legal edits. The
[FREE listing](AGENSI_FREE_LISTING.md) is prepared, not submitted. Agensi's current
[packaging](https://www.agensi.io/docs/packaging),
[FREE pricing](https://www.agensi.io/docs/pricing-your-skill) and
[creator setup](https://www.agensi.io/docs/creator-quickstart) rules were rechecked.
No payout account, paid purchase, listing publication or account mutation was performed.

Remaining hardware/account/owner tasks are enumerated in RELEASE_READINESS_FINAL.md.
Native macOS/Keychain and Linux desktop/package checks remain UNVERIFIED; CI is logic/
compilation evidence only. Main remains unchanged; no public release or tag was created.
