//! Quotis desktop shell: windows, timers and file watching around `quotis-core`.
//! All provider logic lives in the core crate; nothing here knows any provider by name.
//!
//! Locking rules, in one place:
//! - `App` (the engine) is locked only for in-memory work. Never across file, network,
//!   database or window calls.
//! - `PUBLISH` orders snapshots on their way out to the UI and the snapshot file.
//! - `SAVING` makes `settings.json` have a single writer at a time.
//! - `Watch` owns the file watcher, whose calls touch the file system.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use quotis_core::layout::{place, Rect};
use quotis_core::{cache, data_dir, time::now, Engine, Job, ProviderStatus, Settings, Snapshot};
use std::path::PathBuf;
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};
mod persistence;
use tauri::utils::config::WindowEffectsConfig;
use tauri::window::{Effect, EffectState, EffectsBuilder};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewWindow};

const TICK: Duration = Duration::from_secs(15);
/// Local sources are re-read on this many ticks even without a file event, so rolling
/// windows (tokens in the last 5 hours) keep moving while a tool is idle.
const LOCAL_EVERY_TICKS: u32 = 20;
/// The snapshot file is rewritten at least this often so the CLI can tell the app is alive.
const CACHE_HEARTBEAT: Duration = Duration::from_secs(60);
/// Most changed paths kept from one burst of file events.
const MAX_CHANGED: usize = 1024;
/// File events waiting to be handled. Beyond this they are dropped: the periodic local
/// re-read picks up anything missed.
const EVENT_QUEUE: usize = 1024;

static SAVING: Mutex<()> = Mutex::new(());
static PUBLISH: Mutex<Published> = Mutex::new(Published { body: String::new(), cache_written: None, cache_pending: false });

/// What was last sent out, guarded by the lock that also orders sending.
struct Published {
    /// The provider part of the last snapshot sent to the UI and written to disk.
    body: String,
    cache_written: Option<Instant>,
    /// A changed snapshot has not reached the file yet (the write failed): try again at the next emit.
    cache_pending: bool,
}

struct App {
    engine: Engine,
    /// Logical size the UI asked for; what the window currently shows.
    size: (f64, f64),
    /// Destination of the app's latest placement, to distinguish it from user movement.
    placed_position: Option<(i32, i32)>,
    position_dirty: bool,
}

type Shared = Arc<Mutex<App>>;

impl App {
    fn remember_move(&mut self, position: (i32, i32), width: i32) {
        if self.placed_position == Some(position) {
            return;
        }
        self.placed_position = None;
        if self.engine.settings().mode != "notch" {
            self.engine.set_position(Some((position.0 + width / 2, position.1)));
            self.position_dirty = true;
        }
    }
}

struct Watch {
    watcher: RecommendedWatcher,
    watched: Vec<PathBuf>,
}

struct ConfigurationWork(mpsc::SyncSender<()>);

fn unpoisoned<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    // A panic elsewhere must not take the whole app down, so a poisoned lock is used as it is.
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Runs `f` with the app state. `f` must not do I/O.
fn with<R>(app: &AppHandle, f: impl FnOnce(&mut App) -> R) -> R {
    let shared = app.state::<Shared>().inner().clone();
    let mut guard = unpoisoned(&shared);
    f(&mut guard)
}

/// Runs collection jobs outside the lock, records each result, then tells the UI.
fn run_jobs(app: &AppHandle, jobs: Vec<Job>) {
    for job in jobs {
        let outcome = job.run(now());
        with(app, |a| a.engine.finish(&job, outcome, now()));
    }
    emit(app, false);
}

/// One thread per online job, so a slow provider never delays the others.
fn spawn_jobs(app: &AppHandle, jobs: Vec<Job>) {
    if !jobs.is_empty() {
        emit(app, false);
    }
    for job in jobs {
        let handle = app.clone();
        let spawned = std::thread::Builder::new().name("quotis-poll".into()).spawn({
            let job = job.clone();
            move || run_jobs(&handle, vec![job])
        });
        if spawned.is_err() {
            // No thread to run it: report that now, so the provider is not left "refreshing".
            let failed = quotis_core::ProviderError::new(quotis_core::ErrorKind::Internal, "could not start a check");
            with(app, |a| a.engine.finish(&job, Some(Err(failed)), now()));
        }
    }
}

/// Finds out what is installed, off the lock, and records it.
fn probe(app: &AppHandle) {
    let results = with(app, |a| a.engine.probe()).run();
    with(app, |a| a.engine.apply_probe(results));
}

/// Sends the snapshot to the UI and the CLI cache, but only when it changed (or, for the
/// cache, when the heartbeat is due). `force` sends regardless.
///
/// The snapshot is taken and sent under one lock, so a slower thread can never deliver an
/// older snapshot after a newer one.
fn emit(app: &AppHandle, force: bool) {
    let mut published = unpoisoned(&PUBLISH);
    let snap = with(app, |a| a.engine.snapshot(now()));
    let body = serde_json::to_string(&snap.providers).unwrap_or_default();
    let changed = force || body != published.body;
    if changed {
        let _ = app.emit("usage", &snap);
        published.body = body;
        published.cache_pending = true;
    }
    if published.cache_pending || published.cache_written.is_none_or(|t| t.elapsed() >= CACHE_HEARTBEAT) {
        match cache::write(&data_dir(), &snap) {
            Ok(()) => {
                published.cache_written = Some(Instant::now());
                published.cache_pending = false;
            }
            // Still pending: the very next emit tries again, changed or not.
            Err(e) => eprintln!("snapshot cache: {e}"),
        }
    }
}

/// Saves the island's dragged position. The settings are read after the save lock is taken,
/// so a queued writer can never put an older copy back; the "needs saving" mark is cleared
/// only once the file is written, so a failed save is tried again on the next tick.
fn persist_position(app: &AppHandle) -> std::io::Result<()> {
    persistence::flush_position(app.state::<Shared>().inner(), &data_dir())
}

/// Applies the current settings to everything outside the engine: watches, window flags, autostart.
fn configure_sources(app: &AppHandle) {
    let dirs = with(app, |a| a.engine.watch_dirs());
    {
        let state = app.state::<Mutex<Watch>>();
        let mut w = unpoisoned(&state);
        let Watch { watcher, watched } = &mut *w;
        for p in watched.drain(..) {
            let _ = watcher.unwatch(&p);
        }
        for dir in dirs.into_iter().filter(|d| d.is_dir()) {
            match watcher.watch(&dir, RecursiveMode::Recursive) {
                Ok(()) => watched.push(dir),
                Err(e) => eprintln!("watch {}: {e}", dir.display()),
            }
        }
    }
    probe(app);
    let local = with(app, |a| a.engine.local_jobs());
    run_jobs(app, local);
    let online = with(app, |a| a.engine.due_online(now()));
    spawn_jobs(app, online);
}

fn apply(app: &AppHandle) {
    let settings = with(app, |a| a.engine.settings().clone());
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.set_always_on_top(settings.always_on_top);
        #[cfg(target_os = "macos")]
        let _ = w.set_effects(glass_effects(settings.glass, 18.0));
    }
    #[cfg(target_os = "macos")]
    if let Some(w) = app.get_webview_window("settings") {
        let _ = w.set_effects(glass_effects(settings.glass, 16.0));
    }
    {
        use tauri_plugin_autostart::ManagerExt;
        let auto = app.autolaunch();
        let on = auto.is_enabled().unwrap_or(false);
        let result = match (settings.autostart, on) {
            (true, false) => auto.enable(),
            (false, true) => auto.disable(),
            _ => Ok(()),
        };
        if let Err(e) = result {
            eprintln!("autostart: {e}");
        }
    }
    layout(app);
    let _ = app.state::<ConfigurationWork>().0.try_send(());
}

fn rect_of(m: &tauri::Monitor) -> Rect {
    let wa = m.work_area();
    Rect { x: wa.position.x, y: wa.position.y, w: wa.size.width as i32, h: wa.size.height as i32 }
}

/// Picks the monitor the overlay belongs on: the one named in Settings, else (for a dragged
/// island) the one under its saved position, else the primary.
fn target_monitor(w: &WebviewWindow, s: &Settings) -> Option<tauri::Monitor> {
    let monitors = w.available_monitors().unwrap_or_default();
    let named = monitors.iter().find(|m| !s.display.is_empty() && m.name().is_some_and(|n| *n == s.display)).cloned();
    let under_anchor = || s.position.filter(|_| s.mode != "notch").and_then(|(x, y)| monitors.iter().find(|m| rect_of(m).contains(x, y)).cloned());
    named.or_else(under_anchor).or_else(|| w.primary_monitor().ok().flatten()).or_else(|| monitors.into_iter().next())
}

/// Sizes and positions the main window for the current mode. Safe to call at any time:
/// it only touches the window when something actually differs.
fn layout(app: &AppHandle) {
    let Some(w) = app.get_webview_window("main") else { return };
    let (settings, size) = with(app, |a| (a.engine.settings().clone(), a.size));
    let Some(monitor) = target_monitor(&w, &settings) else { return };
    let r = place(rect_of(&monitor), monitor.scale_factor(), size, settings.mode == "notch", settings.position);
    let current = (w.outer_position().ok(), w.outer_size().ok());
    let wanted = (Some(PhysicalPosition::new(r.x, r.y)), Some(PhysicalSize::new(r.w as u32, r.h as u32)));
    if current != wanted {
        with(app, |a| a.placed_position = Some((r.x, r.y)));
        let _ = w.set_position(PhysicalPosition::new(r.x, r.y));
        let _ = w.set_size(PhysicalSize::new(r.w as u32, r.h as u32));
    }
    if !w.is_visible().unwrap_or(true) {
        let _ = w.show();
    }
}

/// Remembers where the user dragged the island (its top-centre), so it returns there.
fn on_moved(app: &AppHandle, w: &tauri::Window, pos: PhysicalPosition<i32>) {
    // Older queued move events must not replace the position the window now occupies.
    if w.outer_position().is_ok_and(|current| current != pos) {
        return;
    }
    let width = w.outer_size().map_or(0, |s| s.width as i32);
    with(app, |a| {
        a.remember_move((pos.x, pos.y), width);
    });
}

fn spawn_event_loop(app: AppHandle, rx: mpsc::Receiver<notify::Result<notify::Event>>) {
    std::thread::spawn(move || {
        while let Ok(first) = rx.recv() {
            let mut changed = Vec::new();
            let mut take = |res: notify::Result<notify::Event>| {
                if let Ok(ev) = res {
                    // Beyond the cap the burst is already known to touch everything worth re-reading.
                    let room = MAX_CHANGED.saturating_sub(changed.len());
                    changed.extend(ev.paths.into_iter().take(room));
                }
            };
            take(first);
            // Tools write in bursts; wait for the burst to end and handle it once.
            // A hard deadline: a tool that never stops writing still gets handled every 300 ms.
            let deadline = Instant::now() + Duration::from_millis(300);
            loop {
                let left = deadline.saturating_duration_since(Instant::now());
                match rx.recv_timeout(left) {
                    Ok(res) if !left.is_zero() => take(res),
                    Ok(res) => {
                        take(res);
                        break;
                    }
                    Err(_) => break,
                }
            }
            changed.sort();
            changed.dedup();
            let jobs = with(&app, |a| a.engine.local_jobs_for(&changed));
            run_jobs(&app, jobs);
        }
    });
}

fn spawn_ticker(app: AppHandle) {
    std::thread::spawn(move || {
        let mut ticks = 0u32;
        loop {
            std::thread::sleep(TICK);
            ticks = ticks.wrapping_add(1);
            let (online, local, dirty) = with(&app, |a| {
                let local = if ticks.is_multiple_of(LOCAL_EVERY_TICKS) { a.engine.local_jobs() } else { Vec::new() };
                (a.engine.due_online(now()), local, a.position_dirty)
            });
            if dirty {
                if let Err(e) = persist_position(&app) {
                    eprintln!("settings: {e}");
                }
            }
            spawn_jobs(&app, online);
            run_jobs(&app, local);
            // Monitors come and go (docking, sleep, resolution changes); keep the overlay anchored.
            let handle = app.clone();
            let _ = app.run_on_main_thread(move || layout(&handle));
        }
    });
}

#[tauri::command]
fn get_usage(app: AppHandle) -> Snapshot {
    with(&app, |a| a.engine.snapshot(now()))
}

#[tauri::command]
fn get_settings(app: AppHandle) -> Settings {
    with(&app, |a| a.engine.settings().clone())
}

#[tauri::command]
async fn get_providers(app: AppHandle) -> Vec<ProviderStatus> {
    let _ = app.state::<ConfigurationWork>().0.try_send(());
    with(&app, |a| a.engine.statuses())
}

#[tauri::command]
async fn save_settings(app: AppHandle, settings: Settings) -> Result<Vec<ProviderStatus>, String> {
    persistence::save(app.state::<Shared>().inner(), &data_dir(), settings, Settings::save, |settings| {
        let statuses = with(&app, |a| a.engine.statuses());
        let _ = app.emit("settings", settings);
        let handle = app.clone();
        if let Err(e) = app.run_on_main_thread(move || apply(&handle)) {
            eprintln!("apply settings: {e}");
        }
        emit(&app, true);
        statuses
    })
    .map_err(|e| e.to_string())
}

/// The UI reports the size its content needs; the window follows.
#[tauri::command]
fn set_size(app: AppHandle, width: f64, height: f64) {
    if !(width.is_finite() && height.is_finite()) {
        return;
    }
    with(&app, |a| a.size = (width.clamp(40.0, 640.0), height.clamp(16.0, 900.0)));
    layout(&app);
}

#[tauri::command]
fn refresh(app: AppHandle) {
    let jobs = with(&app, |a| a.engine.refresh_online());
    spawn_jobs(&app, jobs);
}

#[tauri::command]
fn list_displays(app: AppHandle) -> Vec<String> {
    let Some(w) = app.get_webview_window("main") else { return Vec::new() };
    w.available_monitors().unwrap_or_default().iter().filter_map(|m| m.name().cloned()).collect()
}

#[tauri::command]
async fn quit(app: AppHandle) -> Result<(), String> {
    persist_position(&app).map_err(|e| e.to_string())?;
    app.exit(0);
    Ok(())
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn glass_effects(on: bool, radius: f64) -> Option<WindowEffectsConfig> {
    on.then(|| EffectsBuilder::new().effects([Effect::HudWindow]).state(EffectState::Active).radius(radius).build())
}

#[tauri::command]
async fn open_settings(app: AppHandle) -> Result<(), String> {
    if let Some(w) = app.get_webview_window("settings") {
        let _ = w.unminimize();
        return w.set_focus().map_err(|e| e.to_string());
    }
    let builder = tauri::WebviewWindowBuilder::new(&app, "settings", tauri::WebviewUrl::App("settings.html".into()))
        .title("Quotis Settings")
        .inner_size(420.0, 680.0)
        .min_inner_size(340.0, 420.0)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .always_on_top(true);
    #[cfg(target_os = "macos")]
    let builder = match glass_effects(with(&app, |a| a.engine.settings().glass), 16.0) {
        Some(fx) => builder.effects(fx),
        None => builder,
    };
    builder.build().map(|_| ()).map_err(|e| e.to_string())
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, None))
        .invoke_handler(tauri::generate_handler![get_usage, get_settings, get_providers, save_settings, set_size, refresh, list_displays, open_settings, quit])
        .on_window_event(|window, event| {
            if window.label() != "main" {
                return;
            }
            match event {
                tauri::WindowEvent::Destroyed => window.app_handle().exit(0),
                tauri::WindowEvent::Moved(pos) => on_moved(window.app_handle(), window, *pos),
                tauri::WindowEvent::ScaleFactorChanged { .. } => layout(window.app_handle()),
                _ => {}
            }
        })
        .setup(|app| {
            let handle = app.handle().clone();
            let settings = Settings::load(&data_dir());
            // Bounded: a tool writing faster than Quotis reads cannot grow this without limit.
            let (tx, rx) = mpsc::sync_channel(EVENT_QUEUE);
            let watcher = notify::recommended_watcher(move |event| {
                let _ = tx.try_send(event);
            })?;
            app.manage(Mutex::new(Watch { watcher, watched: Vec::new() }));
            app.manage(Arc::new(Mutex::new(App { engine: Engine::new(settings), size: (200.0, 32.0), placed_position: None, position_dirty: false })));
            let (config_tx, config_rx) = mpsc::sync_channel(1);
            app.manage(ConfigurationWork(config_tx));
            let config_handle = handle.clone();
            std::thread::spawn(move || {
                while config_rx.recv().is_ok() {
                    configure_sources(&config_handle);
                }
            });
            if let Some(w) = app.get_webview_window("main") {
                // Follow the user across virtual desktops and Spaces.
                let _ = w.set_visible_on_all_workspaces(true);
            }
            apply(&handle);
            spawn_event_loop(handle.clone(), rx);
            spawn_ticker(handle);
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            if let tauri::RunEvent::ExitRequested { api, .. } = event {
                if let Err(e) = persist_position(app) {
                    eprintln!("settings could not be saved before exit: {e}");
                    api.prevent_exit();
                }
            }
        });
}
