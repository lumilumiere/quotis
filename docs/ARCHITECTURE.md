# Quotis 2.0 architecture

Quotis is one monitoring engine with two thin shells: a desktop overlay and a read-only CLI.

```
                       +------------------------------+
  tool files, logins   |  crates/quotis-core          |
  provider APIs  --->  |  providers/  adapters        |
                       |  engine.rs   schedule, merge |
                       |  model.rs    schema v1       |
                       |  cache.rs    snapshot.json   |
                       +---------------+--------------+
                                       |
             +-------------------------+--------------------------+
             |                                                    |
  src-tauri (desktop shell)                         crates/quotis-cli (read-only)
  threads, file watcher, window placement           reads snapshot.json, or collects once
  src/*.js  Island / Notch / Panel UI                       |
             |                                      agent skill (text, separate product)
             +--- writes snapshot.json --------------------^
```

## Layout

| Path | Responsibility |
|---|---|
| `crates/quotis-core/src/model.rs` | The usage contract: `Snapshot`, `ProviderSnapshot`, `Metric`, categories, freshness. `SCHEMA_VERSION = 1`. |
| `crates/quotis-core/src/providers/` | One file per adapter. `mod.rs` holds the `Provider` trait and `registry()`. |
| `crates/quotis-core/src/engine.rs` | Per-provider state, scheduling, backoff, merging, snapshot building. No I/O, no threads. |
| `crates/quotis-core/src/time.rs` | Offset-aware timestamp parsing. |
| `crates/quotis-core/src/fsx.rs` | Bounded file access: capped walks that skip links, incremental and tail reads. |
| `crates/quotis-core/src/net.rs` | The only network code: one bounded JSON GET. |
| `crates/quotis-core/src/cache.rs` | Atomic write and validated read of `snapshot.json`. |
| `crates/quotis-core/src/settings.rs` | `settings.json`, migration from 0.x, path resolution. |
| `crates/quotis-core/src/layout.rs` | Window placement geometry, pure and tested. |
| `crates/quotis-cli/` | `quotis-cli` binary. |
| `src-tauri/src/main.rs` | Desktop shell: timers, watcher, commands, window handling. Knows no provider by name. |
| `src/format.js` | Pure view logic (what the pill and rows say). Tested with `node --test`. |
| `src/main.js`, `src/settings.js` | DOM rendering. Text only ever enters the page through `textContent`. |

## How data moves

1. The shell asks the engine for `Job`s: local jobs (read files) and online jobs (one HTTPS GET).
2. Each job runs **outside** the engine lock. A panic in an adapter becomes an error for that provider only.
3. The shell hands the outcome to `Engine::finish`, which merges it. A result created under an older provider configuration is dropped (generation check).
4. `Engine::snapshot(now)` stamps every metric fresh, stale or expired and returns the normalized snapshot.
5. The shell sends the snapshot to the webview and writes `snapshot.json` when it changed, or every 60 seconds as a heartbeat.
6. `quotis-cli` reads that file. It does not talk to the desktop process.

## Scheduling

- Local sources: on a debounced file-system event for that provider's folder, and every 5 minutes so rolling windows keep moving.
- Online sources: every `refresh_min` minutes (default 5) plus up to 10% per-process jitter. Earlier (after 2 minutes) when the tool's files changed or a reset time passed.
- Failures: `Retry-After` is honoured for HTTP 429; otherwise exponential backoff, capped at one hour. One poll in flight per provider.

## The provider contract

```rust
pub trait Provider: Send + Sync {
    fn info(&self) -> &'static Info;                 // id, name, endpoint, documented?, key_env
    fn default_dir(&self) -> PathBuf;
    fn detect(&self, dir: &Path) -> bool;            // cheap, no network
    fn watch_dir(&self, dir: &Path) -> Option<PathBuf>;
    fn collect_local(&self, dir: &Path, now: i64) -> Option<Result<Collected, ProviderError>>;
    fn collect_online(&self, dir: &Path, now: i64) -> Option<Result<Collected, ProviderError>>;
}
```

Adding a provider is one new file plus one line in `registry()`. Settings, the overlay and the CLI list providers from the registry at run time. See [PROVIDER_INTEGRATION_GUIDE.md](PROVIDER_INTEGRATION_GUIDE.md).

## The overlay window

One transparent, undecorated, always-on-top window that is always exactly the size of the island. The page measures the size its content needs and calls `set_size`; Rust picks the monitor and computes the position with `layout::place`. When expanding, the window grows first and the island animates into it; when collapsing, the island shrinks first and the window follows. Nothing animates while idle.

- **Island:** floats, draggable, remembers its top-centre anchor.
- **Notch:** anchored to the top centre of the monitor's work area, square top corners. Not draggable.
- **Panel:** an island that stays expanded.

The work area excludes the taskbar and the macOS menu bar, so the notch sits below a MacBook's camera housing rather than under it. Quotis does not draw inside the menu bar.

## Decisions

Recorded in [adr/README.md](adr/README.md).
