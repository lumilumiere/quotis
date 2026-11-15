//! Quotis desktop shell: windows, timers and file watching around `quotis-core`.
//! All provider logic lives in the core crate; nothing here knows any provider by name.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use quotis_core::layout::{place, Rect};
use quotis_core::{cache, data_dir, time::now, Engine, Job, ProviderStatus, Settings, Snapshot};
use std::path::PathBuf;
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};
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
/// Settings saves run one at a time, so the file and the running state never disagree.
static SAVING: Mutex<()> = Mutex::new(());

struct App {
    engine: Engine,
    watcher: RecommendedWatcher,
    watched: Vec<PathBuf>,
    /// Logical size the UI asked for; what the window currently shows.
    size: (f64, f64),
    /// When the app last moved the window itself, to tell its own moves from a user drag.
    placed_at: Instant,
    position_dirty: bool,
    /// The provider part of the last snapshot sent to the UI and written to disk.
    last_sent: String,
    cache_written: Option<Instant>,
}

type Shared = Arc<Mutex<App>>;

/// Runs `f` with the app state. A panic elsewhere must not take the whole app down, so a
/// poisoned lock is used as it is.
fn with<R>(app: &AppHandle, f: impl FnOnce(&mut App) -> R) -> R {
    let shared = app.state::<Shared>().inner().clone();
    let mut guard = shared.lock().unwrap_or_else(|e| e.into_inner());
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
        let app = app.clone();
        std::thread::spawn(move || run_jobs(&app, vec![job]));
    }
}

/// Sends the snapshot to the UI and the CLI cache, but only when it changed (or, for the
/// cache, when the heartbeat is due). `force` sends regardless.
fn emit(app: &AppHandle, force: bool) {
    let (snap, changed, write) = with(app, |a| {
        let snap = a.engine.snapshot(now());
        let body = serde_json::to_string(&snap.providers).unwrap_or_default();
        let changed = force || body != a.last_sent;
        let write = changed || a.cache_written.is_none_or(|t| t.elapsed() >= CACHE_HEARTBEAT);
        if changed {
            a.last_sent = body;
        }
        if write {
            a.cache_written = Some(Instant::now());
        }
        (snap, changed, write)
    });
    if changed {
        let _ = app.emit("usage", &snap);
    }
    if write {
        if let Err(e) = cache::write(&data_dir(), &snap) {
            eprintln!("snapshot cache: {e}");
        }
    }
}

/// Applies the current settings to everything outside the engine: watches, window flags, autostart.
fn apply(app: &AppHandle) {
    let settings = with(app, |a| {
        for p in a.watched.drain(..) {
            let _ = a.watcher.unwatch(&p);
        }
        for dir in a.engine.watch_dirs() {
            match a.watcher.watch(&dir, RecursiveMode::Recursive) {
                Ok(()) => a.watched.push(dir),
                Err(e) => eprintln!("watch {}: {e}", dir.display()),
            }
        }
        a.engine.settings().clone()
    });
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
    let app = app.clone();
    std::thread::spawn(move || {
        let local = with(&app, |a| a.engine.local_jobs());
        run_jobs(&app, local);
        let online = with(&app, |a| a.engine.due_online(now()));
        spawn_jobs(&app, online);
    });
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
        with(app, |a| a.placed_at = Instant::now());
        let _ = w.set_position(PhysicalPosition::new(r.x, r.y));
        let _ = w.set_size(PhysicalSize::new(r.w as u32, r.h as u32));
    }
    if !w.is_visible().unwrap_or(true) {
        let _ = w.show();
    }
}

/// Remembers where the user dragged the island (its top-centre), so it returns there.
fn on_moved(app: &AppHandle, w: &tauri::Window, pos: PhysicalPosition<i32>) {
    let width = w.outer_size().map_or(0, |s| s.width as i32);
    with(app, |a| {
        let by_user = a.placed_at.elapsed() > Duration::from_millis(400);
        if by_user && a.engine.settings().mode != "notch" {
            a.engine.set_position(Some((pos.x + width / 2, pos.y)));
            a.position_dirty = true;
        }
    });
}

fn spawn_event_loop(app: AppHandle, rx: mpsc::Receiver<notify::Result<notify::Event>>) {
    std::thread::spawn(move || {
        while let Ok(first) = rx.recv() {
            let mut changed = Vec::new();
            let mut take = |res: notify::Result<notify::Event>| {
                if let Ok(ev) = res {
                    // Beyond the cap the burst is already known to touch everything worth re-reading.
                    if changed.len() < MAX_CHANGED {
                        changed.extend(ev.paths)
                    }
                }
            };
            take(first);
            // Tools write in bursts; wait for the burst to end and handle it once.
            let deadline = Instant::now() + Duration::from_millis(300);
            // A hard deadline: a tool that never stops writing still gets handled every 300 ms.
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
            let (online, local, save) = with(&app, |a| {
                let local = if ticks.is_multiple_of(LOCAL_EVERY_TICKS) { a.engine.local_jobs() } else { Vec::new() };
                let save = std::mem::take(&mut a.position_dirty).then(|| a.engine.settings().clone());
                (a.engine.due_online(now()), local, save)
            });
            if let Some(settings) = save {
                if let Err(e) = settings.save(&data_dir()) {
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
    with(&app, |a| a.engine.statuses())
}

#[tauri::command]
async fn save_settings(app: AppHandle, mut settings: Settings) -> Result<Vec<ProviderStatus>, String> {
    settings.sanitize();
    let _one_at_a_time = SAVING.lock().unwrap_or_else(|e| e.into_inner());
    // The window position belongs to the app, not the Settings form.
    settings.position = with(&app, |a| a.engine.settings().position);
    settings.save(&data_dir()).map_err(|e| e.to_string())?;
    let statuses = with(&app, |a| {
        a.engine.set_settings(settings.clone());
        a.engine.statuses()
    });
    let _ = app.emit("settings", &settings);
    let handle = app.clone();
    app.run_on_main_thread(move || apply(&handle)).map_err(|e| e.to_string())?;
    emit(&app, true);
    Ok(statuses)
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
fn quit(app: AppHandle) {
    app.exit(0);
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
            let (tx, rx) = mpsc::channel();
            app.manage(Arc::new(Mutex::new(App {
                engine: Engine::new(settings),
                watcher: notify::recommended_watcher(tx)?,
                watched: Vec::new(),
                size: (200.0, 32.0),
                placed_at: Instant::now(),
                position_dirty: false,
                last_sent: String::new(),
                cache_written: None,
            })));
            if let Some(w) = app.get_webview_window("main") {
                // Follow the user across virtual desktops and Spaces.
                let _ = w.set_visible_on_all_workspaces(true);
            }
            apply(&handle);
            spawn_event_loop(handle.clone(), rx);
            spawn_ticker(handle);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
