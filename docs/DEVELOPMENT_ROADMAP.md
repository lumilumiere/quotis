# Development roadmap

Workstreams from the 2.0 directive, what each depends on, and where it stands. Live status and blockers are in [PROJECT_STATUS.md](PROJECT_STATUS.md).

## Dependency graph

```
A baseline audit ──► B data model ──► C engine ──► D caching ──┬─► E island ──► F notch ──► G platform
                                          │                    ├─► I CLI ──► J agent skill
                                          └─► H providers ─────┘
K testing and security: alongside every stream     L docs and packaging: after C, E, I
M cross-platform release verification: after everything, needs a Mac and signing certificates
```

## Workstreams

| | Workstream | State | Notes |
|---|---|---|---|
| A | Baseline audit and architecture | Done | `BASELINE_AUDIT.md`, `ARCHITECTURE.md`, Codex review 0001 |
| B | Usage data contract | Done | `model.rs`, schema v1 |
| C | Monitoring core refactor | Done | `quotis-core` |
| D | Performance and caching | Done on Windows | Incremental readers, snapshot cache. Memory target missed (WebView2) |
| E | Dynamic Island UI | Done on Windows | Verified in the running app through the webview debugging port |
| F | Notch UI | Done on Windows | Same |
| G | Windows and macOS platform handling | **Partial** | Windows: placement, multi-monitor geometry and scaling are unit-tested; one monitor configuration was run. macOS: written, untested. Fullscreen detection deferred |
| H | Provider stabilisation and expansion | **Partial** | Seven ported, two added. No live-account verification yet |
| I | Local CLI | Done | 13 contract tests |
| J | Agensi skill | Built, not submitted | Passes the local pre-check; needs a seller account |
| K | Testing and security | **Partial** | Four Codex reviews; the last judged the code release-blocking and its findings were repaired, unreviewed. See `SECURITY_REVIEW.md` |
| L | Docs, marketing, packaging | Done as drafts | Legal texts need review |
| M | Cross-platform release verification | **Partial** | CI is green on Windows, Linux, macOS Apple Silicon and Intel. Still needs the overlay run on a Mac and on Linux, an installer smoke test, and signing certificates |

## To reach a 2.0 release

In order:

1. ~~Push the branch and get CI green on all four runners.~~ Done.
1. ~~Have Codex verify the review-0002 fixes.~~ Done (reviews 0003, 0004); have it re-check `fe4c1ef`, the repairs after 0004.
2. Run each supported provider once against a real account and record the result in the provider matrix.
3. Test the overlay on a Mac: launch, Keychain prompt, notch placement on a notched MacBook, Spaces, fullscreen, glass, autostart.
4. Test the overlay on Windows with two monitors at different scale factors, a top-docked taskbar, sleep and resume, and a fullscreen app.
5. Install and uninstall from the built installers on a clean machine.
6. Decide on code signing (R3) and on the undocumented-endpoint risk (R1).
7. Tag `v2.0.0`.

## After 2.0

Ordered by value, each gated on real demand:

- Hide the overlay while a fullscreen app is active (ADR-11).
- Claude Code and Codex local token usage as `local_usage` metrics. Held back because session logs repeat entries, and counting them accurately needs de-duplication that has to be validated against real data.
- OpenAI and Anthropic API usage through admin keys.
- A native (non-webview) collapsed pill, if the memory target matters to users.
- System notifications for warnings.
- An updater, only together with signed builds.
