# Native checks still required

These instructions apply to source/test commit `a99290e`.
They are a reproducible test specification, not evidence that native checks passed.
Use a new OS user/VM, fixture folders and no real provider credentials until authorized.
Record OS/build, CPU/RAM, webview version, display topology/scales, binary SHA256 and UTC.

## Windows

1. Keep generated NSIS/MSI and checksums from `target/release/bundle/`. Verify SHA256;
   builds are unsigned. Use a disposable VM/snapshot without an existing Quotis install.
2. Test each installer independently: install, launch, normal quit, uninstall, and inspect
   installed program/startup entries. Record what happens to user settings. Restore VM
   snapshot between installers. Do not install over the user's working application.
3. Start fixtures with `scripts/with-fixtures.ps1 { ./scripts/windows-runtime.ps1 }`.
   That script targets only its child window/process, tests startup/shutdown/persistence,
   and measures its process tree. Its three launch modes do not prove visual appearance.
4. Manually inspect Island, Notch and Panel; expand/collapse, Escape, Settings open/close,
   themes, glass/opacity, pinned provider, warning thresholds and scrolling on a short
   display. Check keyboard focus, long labels and text-only hostile provider strings.
5. Use two monitors with negative secondary origin and scales 100%,150%,200%. Select each
   display; drag Island/Panel within Quotis, save while moving, quit immediately, restart.
   Test dock/unplug and sleep/resume. Notch must remain anchored and all rows reachable.
6. Use malformed/unknown fixture logs and a newly created locked log to check partial and
   permission errors. Keep last values alongside errors. For stale/reset behavior use
   captured fixture snapshots with old observations and CLI; native UI behavior must also
   be observed over an actual failure/age interval before certifying it.
7. In a fresh safe VM only, install 0.2.1, seed disabled-provider settings in fake directories,
   upgrade to 2.0 and check migration, position, autostart and provider choices. Keep a VM
   snapshot so failures are reversible. Never use the current machine as the upgrade target.

## macOS

Build/test on both Apple Silicon and Intel where available. Run the same visual/multi-display
checks, including Retina scaling, menu-bar work area, camera notch, Spaces/fullscreen,
visible-on-all-workspaces behavior and glass effects. Record sandbox/privacy prompts.
CI compilation is not native validation. Install/uninstall DMG/app only in an isolated user;
test signing/notarization separately with actual certificates before claiming either.

Keychain is a separate gate. In an isolated test user and only with explicit authorization,
check missing item, approved access, denied access, dismissed/unanswered prompt (60s timeout)
and child-process cleanup for `Claude Code-credentials`. Stop on denial. Never change the
primary user's Keychain search list, print stored values, or reuse a denied access attempt.
No Claude API call/session is needed for a fixture/prompt test.

## Linux

Use a native session with WebKitGTK4.1/appindicator dependencies. Exercise X11 and Wayland
where supported, fractional scaling, negative monitor origin, compositor transparency,
always-on-top/workspace behavior, Settings sizing, normal exit and desktop launch paths.
Run package install/remove in disposable distro VMs. Record unsupported window-manager
operations rather than treating ignored native errors as successful behavior.

## Accounts and final sign-off

For each provider, obtain permission for a named test account/local source. Enable only
that provider, run `quotis-cli status --live --json`, compare with its actual usage display
at the same UTC time, and record source, windows/resets, values, qualifications and errors
in PROVIDER_MATRIX.md. Do not paste credentials. Permission denial stops the check.

Review licence/privacy wording, naming and undocumented-endpoint terms. Submit the skill
as FREE only with the unreleased/live-unverified disclosures or after verifying the release.
Agensi account/profile, upload review and MCP delivery must be completed separately; do
not configure payouts or buy services for the FREE debut. Public release requires explicit
decisions on every gate in RELEASE_READINESS_FINAL.md, without converting UNVERIFIED to PASS.
