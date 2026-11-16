# Performance report

## Final repaired Windows build, 2026-11-16

Measured production source `a99290e37365b09df1435712a5d6afa7d5df91d9`, binary SHA256
`E98B67BB581DC2C6C58114662EA7E93222459630E43860EEAEB6B45D58610142`.
[Raw retained measurement](validation/windows-runtime-a99290e.json),
[reproduction script](../scripts/windows-runtime.ps1). Fixture sources only; all account
providers disabled. No provider requests, real credentials or global input injection.

| Metric | Measured result |
|---|---|
| First snapshot after launch: Island / Notch / Panel | 3,238 / 3,732 / 2,338 ms |
| Idle tree CPU over 60.39 s | 0.3125 CPU-seconds; **0.517% of one core**, 0.065% of 8 logical processors |
| Idle private memory, app+WebView tree | **144.00 MiB** |
| Summed working set | 341.76 MiB; shared pages may be counted more than once |
| Normal local-refresh tree CPU delta | 0.015625 CPU-seconds over the 3 s observation plus process sampling |
| Forced local-read failure | Last 84-token values retained with Unavailable; 144.23 MiB private memory, 0.171875 CPU-seconds over 18 s plus sampling |
| Startup/shutdown/persistence | All 3 modes produced 42-token fixture snapshots and exited0; targeted native moves persisted in Island/Panel |

Machine: Intel Core i5-1135G7, 4 cores/8 threads, 15.7 GiB visible RAM, Windows 11 Pro
10.0.26300. One display reported by WinForms at 1600x900; that can be DPI-virtualized.
No scaling/monitor changes were performed. Release profile remains size-optimized/LTO.

The script launches each child with a sealed home/config and no autostart/topmost setting,
waits 20 s before the idle sample, then measures the app and descendants twice 60 s apart.
A tiny append exercises normal refresh. A new exclusively locked fixture log produces a
local I/O error; the last metrics and error are retained in the raw snapshot. Window moves
and close target only this process's HWND. No mouse or keyboard input is sent globally.

An earlier repaired-build sample before the final movement fix measured 141.81 MiB and
0.336% one-core CPU; its persistence test failed and led to the movement repair. These are
single-machine observations with background load/sampling overhead, not stable benchmarks
or a controlled before/after comparison. Startup includes OS/webview cache variation.

The approximately 144 MiB total is consistent with the prior approximately 145 MB result;
120MB is not an absolute release gate. No long-duration leak, energy, frame-rate, real
network-refresh, hanging-kernel-I/O or native macOS/Linux measurement is claimed. The
failure sample is a bounded local-read error, not proof that arbitrary I/O can be cancelled.

The remaining sections are the **historical pre-hardening measurement** and are retained
to show their different code revision/setup. Their "not re-measured" note is superseded
by this section only for the specific final metrics above.

---

Measured on one Windows machine with fixture data. These are measurements of that setup, not guarantees. Nothing was measured on macOS or Linux, and v0.2.1 was not profiled before the rewrite, so there is no before/after comparison.

## Setup

| | |
|---|---|
| Machine | Laptop, Intel Core i5-1135G7 (4 cores / 8 threads, 2.4 GHz), 15.7 GB RAM |
| OS | Windows 11 Pro 10.0.26300 |
| Webview | Microsoft Edge WebView2 157.0.4341.46 |
| Build | `quotis-2.0` branch at `f0b3ea5`, before the review fixes (not re-measured afterwards), release profile (`opt-level = "s"`, LTO, 1 codegen unit, stripped) |
| Date | 2026-11-15 |
| Data | Fixture folders: Codex session log with rate limits, Gemini token log. Other providers pointed at an empty folder, so **no network requests were made** during measurement |
| Mode | Island, collapsed, matte, dark |
| Other load | A game (League of Legends) was running in the foreground. CPU time is measured per process, so this adds noise to timings but not to CPU-seconds |

## Method

`measure.ps1` (a throwaway PowerShell script) starts the app, waits for `snapshot.json` to appear, lets it settle for 20 seconds, then samples the whole process tree (the app and its WebView2 child processes) twice, 60 seconds apart. CPU is the difference in accumulated CPU time. Memory is the sum over the tree at the second sample.

## Results

| Metric | Run 1 | Run 2 | Target | Met? |
|---|---|---|---|---|
| Time from launch to first snapshot written | 1,666 ms | 649 ms | minimise | n/a |
| Idle CPU, whole tree, % of one core | 0.47% | 0.34% | under 1% | **Yes** |
| Idle CPU, whole tree, % of all 8 threads | 0.06% | 0.04% | | |
| Idle CPU seconds in 60 s: whole tree / app process alone | 0.281 / 0.031 | 0.203 / not recorded | | |
| Private memory, whole tree (7 processes) | 143.9 MB | 145.1 MB | under 120 MB | **No** |
| Private memory, Quotis process alone | 5.4 MB | 5.4 MB | | |
| Working set, summed over the tree | 355.8 MB | 349.9 MB | | see note |

Run 1 was the first launch of this build (cold); run 2 followed immediately (warm).

**Memory.** The Quotis process itself uses about 5 MB of private memory. The other ~140 MB is WebView2's six processes (browser, GPU, renderer, utility, crash handler). The 120 MB target is missed by about 25 MB on this machine. The summed working set (about 350 MB) overstates real use because pages shared between the WebView2 processes are counted once per process.

**What Quotis controls here:** the page has no framework and no images, and does nothing while idle. What it does not control: the fixed cost of a WebView2 instance. Getting under 120 MB would mean not using a webview for the collapsed pill (a native window), which is a larger change than 2.0 took on.

| CLI (release build, 10 runs each) | Median | Max |
|---|---|---|
| `quotis-cli status` (reads the snapshot file) | 14 ms | 228 ms (first run) |
| `quotis-cli snapshot --live --offline` (reads the fixture logs itself) | 16 ms | 21 ms |

| Size | |
|---|---|
| `quotis.exe` (desktop, not bundled) | 8.9 MB |
| `quotis-cli.exe` | 3.2 MB |
| Frontend bundle | 8.0 kB JS + 18.7 kB CSS (overlay), 3.6 kB JS (Settings), plus 15.7 kB shared Tauri API |

## What changed that affects cost (by design, not measured against 0.2.1)

| 0.2.1 | 2.0 |
|---|---|
| Blur thread woke every 50 ms and sampled the screen | Removed. No screen access at all |
| Every file event re-read up to 20 whole Codex logs and whole token logs | Reads the last 256 KB of the newest logs; token logs are read from a saved offset |
| UI re-rendered every 15 s | Re-renders on change, plus a 30 s tick for countdowns |
| SQLite query on every UI update, under the state lock | On file change and every 5 minutes, outside the lock |

## Not measured

- **Animation frame rate.** The expand/collapse transition was checked for correct start and end states, not profiled for dropped frames.
- **Energy.** No power measurement was taken.
- **Cost with real providers and large real logs.** Fixture logs are tiny. Network polls add one HTTPS request per online provider per 5 minutes.
- **Collection overhead on large log folders.** The incremental reader is unit-tested for correctness, not benchmarked.
- **macOS and Linux.** Nothing.
- **Glass mode and expanded state** idle cost.

## Reproducing

Build with `npx tauri build --no-bundle`, point `QUOTIS_DATA_DIR` at a folder containing a `settings.json`, and sample the process tree with any process monitor after the app has been idle for 20 seconds.
