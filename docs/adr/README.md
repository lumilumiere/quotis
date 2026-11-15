# Architecture decision records

Short records of the decisions that shaped Quotis 2.0. Newest last.

## ADR-1: Keep Rust and Tauri 2

**Decision:** stay on Rust + Tauri 2 with the system webview.
**Why:** the baseline already met the "lightweight" goal with it, and nothing measured argued for a change. Electron would add a bundled browser.
**Cost:** rendering differs per OS webview (WebView2, WKWebView, WebKitGTK); macOS behaviour has to be tested on a Mac.

## ADR-2: Three crates, not five

**Decision:** `quotis-core`, `quotis-cli`, `src-tauri`. Models, providers and platform geometry live in the core crate as modules.
**Why:** the directive's five-crate sketch was illustrative. Separate crates for models and providers would add manifests and no boundary the module system does not already give.
**Revisit when:** a second consumer needs the model without the collectors (for example a WASM build).

## ADR-3: The engine does no I/O

**Decision:** the engine returns `Job`s; shells run them outside the lock and report back.
**Why:** the baseline held its one mutex across SQLite queries and log scans. With this split a slow disk or network can never block the UI thread or other providers, and the engine is testable with scripted providers.

## ADR-4: A snapshot file is the IPC

**Decision:** the desktop app writes `snapshot.json` in the per-user config folder (mode 0600 on Unix, per-user ACL on Windows); the CLI reads it.
**Why:** no socket, no port, no pipe protocol to secure or version. Same-user access control comes from the OS file permissions. Atomic rename means a reader never sees a torn file.
**Cost:** data is up to a refresh interval old; `--live` exists for when that is not good enough. A heartbeat rewrite every 60 seconds lets the CLI tell a live app from a dead one.

## ADR-5: CLI binary is `quotis-cli`

**Decision:** the CLI is named `quotis-cli`, not `quotis`.
**Why:** the desktop binary is already `quotis` and Windows file names are case-insensitive, so `Quotis.exe` and `quotis.exe` cannot share a folder. The desktop app is a GUI-subsystem binary on Windows and cannot print to a console, so one binary could not serve both roles.

## ADR-6: Remove the Windows screen-sampled blur

**Decision:** delete the GDI backdrop sampler. Default look is matte; "glass" is translucent CSS, with native vibrancy on macOS only.
**Why:** the sampler read the screen about 20 times a second, including strips outside the window, contradicting the README. Windows has no compositor blur that follows a rounded, non-rectangular shape, so there is no cheap honest replacement.
**Cost:** Windows users lose real background blur. This is an intentional change from 0.2.

## ADR-7: Hand-written timestamp parser, no date crate

**Decision:** fix `parse_ts` in place (offsets, validation) rather than add `chrono` or `time`.
**Why:** the need is one function: RFC 3339 to unix seconds. Thirty lines with tests beat a dependency.

## ADR-8: Expired quota is qualified, not reset

**Decision:** when a reset time passes, the metric keeps its last value and is marked `expired`; the UI shows "reset · updating…" and the engine re-polls early.
**Why:** the baseline displayed 100% left without asking the provider. Quotis cannot know the new value until the provider says so.

## ADR-9: API-key providers read the key from the environment and are off by default

**Decision:** OpenRouter and DeepSeek use `OPENROUTER_API_KEY` / `DEEPSEEK_API_KEY`, are disabled until the user turns them on, and Quotis never stores the key.
**Why:** storing keys would make Quotis a credential store. Opt-in avoids sending a key the user set for another purpose without their say.
**Cost:** a GUI app sees only the environment it was launched with; documented in the install guide.

## ADR-10: No general "run this for me" surface

**Decision:** the CLI has five fixed read commands and no plugin, shell-out or file-path argument. The webview's capabilities are events, window drag and window close.
**Why:** the CLI is meant to be called by coding agents; anything it could be talked into doing must be harmless.

## ADR-11: Fullscreen-app detection is deferred

**Decision:** no "hide while a fullscreen app is running" in 2.0.
**Why:** it needs a per-OS native call (`SHQueryUserNotificationState` on Windows) and the overlay already loses z-order to exclusive-fullscreen apps. The "Keep on top" switch is the manual control.
**Revisit when:** users report the overlay covering borderless-fullscreen games.
