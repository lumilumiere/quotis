# Quotis 2.0 project status

Checkpoint written 2026-11-15 when the session's usage limit was reached. Work is **not** continuing in the background.

## Where things are

- Branch: `quotis-2.0` (local only, not pushed), based on `main` at `f80bc40` (tag `v0.2.1`).
- Orca run: `run_21d09bc275ac`. One Codex worker ran (baseline audit) and was released. No workers are live.
- Commits are authored with a command-local placeholder identity (`Quotis Agent (Claude) <claude@quotis.invalid>`) because no git identity is configured. Rewrite authorship before pushing if you want your own.

## Done and verified on Windows 11

- Baseline: v0.2.1 builds; 8 Rust tests pass, 1 live test ignored.
- Cargo workspace: `crates/quotis-core`, `crates/quotis-cli`, `src-tauri`.
- Core: normalized usage model (schema v1), offset-aware timestamp parser, bounded incremental file readers that do not follow symlinks or junctions, provider adapter trait and registry, engine with backoff, jitter, freshness (fresh / stale / expired), per-provider configuration generation, atomic snapshot cache.
- Providers ported: Claude Code, Codex, GitHub Copilot, Cursor, Gemini CLI, Qwen Code, opencode. New, opt-in, built from official docs and **not verified against a live account**: OpenRouter, DeepSeek.
- CLI `quotis-cli`: `status`, `snapshot`, `providers`, `diagnose`, `export`; exit codes 0/2/3/4/5/6/7.
- Desktop shell rewritten on the core. Island, notch and panel modes; Windows screen-sampling blur removed.
- Tests: 87 Rust tests (core 59, engine 16, CLI 12) and 14 frontend tests pass. `cargo clippy -D warnings` clean. `npm audit` reports 0 vulnerabilities.
- Runtime check with fixture data: the debug app starts, shows the collapsed pill top-centre, writes the snapshot, and `quotis-cli status` reads it.
- Independent review: `docs/reviews/0001-baseline-codex.md` (Codex, worker commit `c53003a`), 18 findings, none critical or high. Fixes for its findings are in the code with regression tests.

## Not done

- Expand / collapse, hover, drag, notch and panel modes, Settings window: implemented, **not yet exercised in the running app**. A WebView2 debugging-port harness was about to be used for this.
- Codex review of the 2.0 implementation commit (only the baseline was reviewed).
- No test against real provider accounts was run in this session.
- macOS and Linux: nothing built or run. All macOS behaviour is unverified.
- Fullscreen-app detection on Windows is not implemented.
- Docs not written: BASELINE_AUDIT, ARCHITECTURE, THREAT_MODEL, RISK_REGISTER, DEVELOPMENT_ROADMAP, PROVIDER_MATRIX, PROVIDER_INTEGRATION_GUIDE, PERFORMANCE_REPORT, SECURITY_REVIEW, PRIVACY_MODEL, ADRs. README still describes v0.2.1.
- No performance measurements taken.
- CI workflow not updated (it still points at `src-tauri/Cargo.toml` and the old lockfile path, so the release workflow is currently broken on this branch).
- Agensi skill, commercial package, release build, installers, signing: not started. Agensi packaging and review rules were researched only.

## Incident to know about

While testing click-to-expand, two synthetic left clicks were sent at the top centre of the primary screen. League of Legends was in the foreground and received them instead of the overlay. Input injection was stopped; the cursor position was restored after each click.

## Next action

Drive the running debug app over the WebView2 debugging port to verify expand / collapse and the three modes, then dispatch a Codex review of the implementation commit, then fix CI.
