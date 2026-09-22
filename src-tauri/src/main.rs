
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::utils::config::WindowEffectsConfig;
use tauri::window::{Effect, EffectState, EffectsBuilder};
use tauri::{AppHandle, Emitter, Manager};

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
struct ProviderCfg {
    enabled: bool,
    dir: String,
}

impl Default for ProviderCfg {
    fn default() -> Self {
        Self { enabled: true, dir: String::new() }
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
struct Settings {
    claude: ProviderCfg,
    codex: ProviderCfg,
    gemini: ProviderCfg,
    claude_refresh_min: u64,
    always_on_top: bool,
    show_used: bool,
    glass_opacity: u8,
    blur: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            claude: ProviderCfg::default(),
            codex: ProviderCfg::default(),
            gemini: ProviderCfg::default(),
            claude_refresh_min: 5,
            always_on_top: true,
            show_used: false,
            glass_opacity: 55,
            blur: true,
        }
    }
}

fn settings_path(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_config_dir().ok().map(|d| d.join("settings.json"))
}

fn load_settings(app: &AppHandle) -> Settings {
    settings_path(app)
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

#[derive(Clone)]
struct Dirs {
    claude: PathBuf,
    codex: PathBuf,
    gemini: PathBuf,
}

fn home() -> PathBuf {
    std::env::home_dir().unwrap_or_default()
}

fn resolve(cfg: &ProviderCfg, env: Option<&str>, default: &str) -> PathBuf {
    let dir = cfg.dir.trim();
    if let Some(rest) = dir.strip_prefix('~') {
        return home().join(rest.trim_start_matches(['/', '\\']));
    }
    if !dir.is_empty() {
        return PathBuf::from(dir);
    }
    env.and_then(std::env::var_os).map(PathBuf::from).unwrap_or_else(|| home().join(default))
}

impl Dirs {
    fn from(s: &Settings) -> Self {
        Dirs {
            claude: resolve(&s.claude, Some("CLAUDE_CONFIG_DIR"), ".claude"),
            codex: resolve(&s.codex, Some("CODEX_HOME"), ".codex"),
            gemini: resolve(&s.gemini, None, ".gemini"),
        }
    }
    fn claude_logs(&self) -> PathBuf { self.claude.join("projects") }
    fn codex_logs(&self) -> PathBuf { self.codex.join("sessions") }
    fn gemini_logs(&self) -> PathBuf { self.gemini.join("tmp") }
}

#[derive(Serialize)]
struct ToolStatus {
    path: String,
    found: bool,
}

#[derive(Serialize)]
struct Status {
    claude: ToolStatus,
    codex: ToolStatus,
    gemini: ToolStatus,
}

fn detect(d: &Dirs) -> Status {
    let st = |p: &Path, found: bool| ToolStatus { path: p.display().to_string(), found };
    Status {
        claude: st(&d.claude, claude_credentials(&d.claude).is_some()),
        codex: st(&d.codex, d.codex_logs().is_dir()),
        gemini: st(&d.gemini, d.gemini_logs().is_dir()),
    }
}

#[derive(Clone, Serialize)]
struct Limit {
    used_percent: f64,
    resets_at: i64,
}

#[derive(Clone, Serialize, Default)]
struct Row {
    connected: bool,
    five_hour: Option<Limit>,
    weekly: Option<Limit>,
    tokens_5h: Option<u64>,
    tokens_24h: Option<u64>,
    status: Option<String>,
}

#[derive(Clone, Serialize, Default)]
struct Snapshot {
    claude: Row,
    codex: Row,
    gemini: Row,
}

struct State {
    settings: Settings,
    dirs: Dirs,
    snap: Snapshot,
    gemini_files: HashMap<PathBuf, Vec<(i64, u64)>>,
    claude_dirty: bool,
    last_poll: Option<Instant>,
    claude_backoff: u64,
    watcher: RecommendedWatcher,
    watched: Vec<PathBuf>,
}

type Shared = Mutex<State>;

fn now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

fn parse_ts(s: &str) -> Option<i64> {
    let n = |r: std::ops::Range<usize>| s.get(r)?.parse::<i64>().ok();
    let (y, m, d) = (n(0..4)?, n(5..7)?, n(8..10)?);
    let (hh, mm, ss) = (n(11..13)?, n(14..16)?, n(17..19)?);

    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * ((m + 9) % 12) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some((era * 146097 + doe - 719468) * 86400 + hh * 3600 + mm * 60 + ss)
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for p in rd.flatten().map(|e| e.path()) {
        if p.is_dir() { walk(&p, out) } else { out.push(p) }
    }
}

fn is_jsonl(p: &Path) -> bool {
    p.extension().is_some_and(|e| e == "jsonl")
}

fn claude_credentials(root: &Path) -> Option<Value> {
    let text = std::fs::read_to_string(root.join(".credentials.json")).ok().or_else(keychain)?;
    let v: Value = serde_json::from_str(&text).ok()?;
    v.get("claudeAiOauth").is_some().then_some(v)
}

#[cfg(target_os = "macos")]
fn keychain() -> Option<String> {
    let out = std::process::Command::new("security")
        .args(["find-generic-password", "-s", "Claude Code-credentials", "-w"])
        .output()
        .ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

#[cfg(not(target_os = "macos"))]
fn keychain() -> Option<String> {
    None
}

enum ClaudeErr {
    NotSignedIn,
    Expired,
    RateLimited(Option<u64>),
    Unavailable(&'static str),
}

fn fetch_claude(root: &Path) -> Result<Row, ClaudeErr> {
    let creds = claude_credentials(root).ok_or(ClaudeErr::NotSignedIn)?;
    let oauth = &creds["claudeAiOauth"];
    let token = oauth.get("accessToken").and_then(Value::as_str).ok_or(ClaudeErr::NotSignedIn)?;

    if oauth.get("expiresAt").and_then(Value::as_i64).is_some_and(|ms| ms / 1000 < now()) {
        return Err(ClaudeErr::Expired);
    }
    let mut resp = ureq::get("https://api.anthropic.com/api/oauth/usage")
        .config()
        .http_status_as_error(false)
        .build()
        .header("Authorization", &format!("Bearer {token}"))
        .header("anthropic-beta", "oauth-2025-04-20")
        .call()
        .map_err(|e| {
            eprintln!("claude usage request failed: {e}");
            ClaudeErr::Unavailable("offline")
        })?;
    match resp.status().as_u16() {
        200 => {}
        429 => {
            let retry = resp.headers().get("retry-after").and_then(|v| v.to_str().ok()?.trim().parse().ok());
            return Err(ClaudeErr::RateLimited(retry));
        }
        401 | 403 => return Err(ClaudeErr::Expired),
        code => {
            eprintln!("claude usage request: HTTP {code}");
            return Err(ClaudeErr::Unavailable("unavailable"));
        }
    }
    let body = resp.body_mut().read_to_string().map_err(|_| ClaudeErr::Unavailable("bad response"))?;
    let mut row = parse_claude_usage(&body).ok_or(ClaudeErr::Unavailable("bad response"))?;
    row.connected = true;
    Ok(row)
}

fn parse_claude_usage(body: &str) -> Option<Row> {
    let v: Value = serde_json::from_str(body).ok()?;
    let lim = |k: &str| {
        let w = v.get(k)?;
        Some(Limit {
            used_percent: w.get("utilization")?.as_f64()?,
            resets_at: w.get("resets_at").and_then(Value::as_str).and_then(parse_ts).unwrap_or(0),
        })
    };
    Some(Row { five_hour: lim("five_hour"), weekly: lim("seven_day"), ..Default::default() })
}

fn poll_claude(app: &AppHandle) {
    let root = {
        let state = app.state::<Shared>();
        let s = state.lock().unwrap();
        if !s.settings.claude.enabled {
            return;
        }
        s.dirs.claude.clone()
    };
    let result = fetch_claude(&root);
    {
        let state = app.state::<Shared>();
        let mut s = state.lock().unwrap();
        s.last_poll = Some(Instant::now());
        s.claude_dirty = false;
        if s.settings.claude.enabled && s.dirs.claude == root {
            let has_data = s.snap.claude.five_hour.is_some() || s.snap.claude.weekly.is_some();
            let problem = |msg: &str| Row { connected: true, status: Some(msg.into()), ..Default::default() };
            match result {
                Ok(row) => {
                    s.snap.claude = row;
                    s.claude_backoff = 0;
                }
                Err(ClaudeErr::NotSignedIn) => s.snap.claude = Row::default(),
                Err(ClaudeErr::Expired) => s.snap.claude = problem("login expired, open Claude Code"),

                Err(ClaudeErr::RateLimited(retry)) => {
                    s.claude_backoff = retry.unwrap_or((s.claude_backoff * 2).max(300)).min(3600);
                    if !has_data {
                        s.snap.claude = problem("rate limited, retrying later");
                    }
                }
                Err(ClaudeErr::Unavailable(msg)) => {
                    if !has_data {
                        s.snap.claude = problem(msg);
                    }
                }
            }
        }
    }
    emit(app);
}

fn codex_row(logs: &Path) -> Row {
    let mut files = Vec::new();
    walk(logs, &mut files);
    let newest = files
        .into_iter()
        .filter(|p| is_jsonl(p))
        .max_by_key(|p| p.metadata().and_then(|m| m.modified()).unwrap_or(UNIX_EPOCH));
    let mut row = newest
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|t| parse_codex_limits(&t))
        .unwrap_or_else(|| Row { status: Some("no limit data yet".into()), ..Default::default() });
    row.connected = true;
    row
}

fn parse_codex_limits(text: &str) -> Option<Row> {
    let line = text.lines().rev().find(|l| l.contains("\"rate_limits\""))?;
    let v: Value = serde_json::from_str(line).ok()?;
    let rl = v.pointer("/payload/rate_limits")?;
    let lim = |k: &str| {
        let w = rl.get(k)?;
        Some(Limit { used_percent: w.get("used_percent")?.as_f64()?, resets_at: w.get("resets_at")?.as_i64()? })
    };
    Some(Row { five_hour: lim("primary"), weekly: lim("secondary"), ..Default::default() })
}

fn gemini_entries(path: &Path) -> Vec<(i64, u64)> {
    let Ok(text) = std::fs::read_to_string(path) else { return Vec::new() };
    text.lines()
        .filter(|l| l.contains("\"tokens\""))
        .filter_map(|l| {
            let v: Value = serde_json::from_str(l).ok()?;
            Some((parse_ts(v.get("timestamp")?.as_str()?)?, v.pointer("/tokens/total")?.as_u64()?))
        })
        .collect()
}

fn scan_gemini(logs: &Path) -> HashMap<PathBuf, Vec<(i64, u64)>> {
    let mut files = Vec::new();
    walk(logs, &mut files);
    let cutoff = SystemTime::now() - Duration::from_secs(24 * 3600);
    files
        .into_iter()
        .filter(|p| is_jsonl(p) && p.metadata().and_then(|m| m.modified()).is_ok_and(|t| t > cutoff))
        .map(|p| {
            let e = gemini_entries(&p);
            (p, e)
        })
        .collect()
}

fn emit(app: &AppHandle) {
    let snap = {
        let state = app.state::<Shared>();
        let mut s = state.lock().unwrap();
        if s.snap.gemini.connected {
            let t = now();
            let sum = |secs: i64| {
                s.gemini_files.values().flatten().filter(|(ts, _)| t - ts < secs).map(|(_, n)| n).sum::<u64>()
            };
            let (h5, h24) = (sum(5 * 3600), sum(24 * 3600));
            s.snap.gemini.tokens_5h = Some(h5);
            s.snap.gemini.tokens_24h = Some(h24);
        }
        s.snap.clone()
    };
    let _ = app.emit("usage", snap);
}

fn apply(app: &AppHandle, poll_claude_now: bool) {
    {
        let state = app.state::<Shared>();
        let mut guard = state.lock().unwrap();
        let s = &mut *guard;
        s.dirs = Dirs::from(&s.settings);
        for p in s.watched.drain(..) {
            let _ = s.watcher.unwatch(&p);
        }
        let (codex_on, gemini_on) =
            (s.settings.codex.enabled && s.dirs.codex_logs().is_dir(), s.settings.gemini.enabled && s.dirs.gemini_logs().is_dir());
        let targets = [
            (s.settings.claude.enabled, s.dirs.claude_logs()),
            (codex_on, s.dirs.codex_logs()),
            (gemini_on, s.dirs.gemini_logs()),
        ];
        for (on, dir) in targets {
            if !on || !dir.is_dir() {
                continue;
            }
            match s.watcher.watch(&dir, RecursiveMode::Recursive) {
                Ok(()) => s.watched.push(dir),
                Err(e) => eprintln!("watch {}: {e}", dir.display()),
            }
        }

        if poll_claude_now || !s.settings.claude.enabled {
            s.snap.claude = Row::default();
        }
        s.snap.codex = if codex_on { codex_row(&s.dirs.codex_logs()) } else { Row::default() };
        s.gemini_files = if gemini_on { scan_gemini(&s.dirs.gemini_logs()) } else { HashMap::new() };
        s.snap.gemini = Row { connected: gemini_on, ..Default::default() };
        if let Some(w) = app.get_webview_window("main") {
            let _ = w.set_always_on_top(s.settings.always_on_top);
        }
        #[cfg(not(windows))]
        for label in ["main", "settings"] {
            if let Some(w) = app.get_webview_window(label) {
                let _ = w.set_effects(glass_effects(s.settings.blur));
            }
        }
    }
    emit(app);
    if poll_claude_now {
        let app = app.clone();
        std::thread::spawn(move || poll_claude(&app));
    }
}

fn spawn_event_loop(app: AppHandle, rx: mpsc::Receiver<notify::Result<notify::Event>>) {
    std::thread::spawn(move || {
        while let Ok(first) = rx.recv() {
            let mut changed = Vec::new();
            let mut take = |res: notify::Result<notify::Event>| if let Ok(ev) = res { changed.extend(ev.paths) };
            take(first);
            let deadline = Instant::now() + Duration::from_millis(300);
            while let Ok(res) = rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) { take(res) }
            changed.sort();
            changed.dedup();

            let d = app.state::<Shared>().lock().unwrap().dirs.clone();
            let (claude_logs, codex_logs, gemini_logs) = (d.claude_logs(), d.codex_logs(), d.gemini_logs());
            let codex = changed.iter().any(|p| p.starts_with(&codex_logs)).then(|| codex_row(&codex_logs));
            let gemini: Vec<_> = changed
                .iter()
                .filter(|p| p.starts_with(&gemini_logs) && is_jsonl(p))
                .map(|p| (p.clone(), gemini_entries(p)))
                .collect();

            let state = app.state::<Shared>();
            let mut s = state.lock().unwrap();
            if changed.iter().any(|p| p.starts_with(&claude_logs)) {
                s.claude_dirty = true;
            }
            if let Some(row) = codex.filter(|_| s.snap.codex.connected) {
                s.snap.codex = row;
            }
            if s.snap.gemini.connected {
                s.gemini_files.extend(gemini);
            }
            drop(s);
            emit(&app);
        }
    });
}

fn spawn_ticker(app: AppHandle) {
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(15));
        let due = {
            let state = app.state::<Shared>();
            let s = state.lock().unwrap();
            let every = (s.settings.claude_refresh_min.clamp(1, 60) * 60).max(s.claude_backoff);
            let while_active = 120.max(s.claude_backoff);
            s.settings.claude.enabled
                && s.last_poll.is_none_or(|t| {
                    let secs = t.elapsed().as_secs();
                    secs >= every || (s.claude_dirty && secs >= while_active)
                })
        };
        if due {
            poll_claude(&app);
        } else {
            emit(&app);
        }
    });
}

#[tauri::command]
fn get_usage(state: tauri::State<'_, Shared>) -> Snapshot {
    state.lock().unwrap().snap.clone()
}

#[tauri::command]
fn get_settings(state: tauri::State<'_, Shared>) -> Settings {
    state.lock().unwrap().settings.clone()
}

#[tauri::command]
async fn get_status(app: AppHandle) -> Status {
    let dirs = app.state::<Shared>().lock().unwrap().dirs.clone();
    detect(&dirs)
}

#[tauri::command]
async fn save_settings(app: AppHandle, mut settings: Settings) -> Result<Status, String> {
    settings.claude_refresh_min = settings.claude_refresh_min.clamp(1, 60);
    settings.glass_opacity = settings.glass_opacity.min(100);
    let path = settings_path(&app).ok_or("no config folder")?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())?;
    let dirs = Dirs::from(&settings);
    let claude_changed = {
        let state = app.state::<Shared>();
        let mut s = state.lock().unwrap();
        let old = &s.settings.claude;
        let changed = old.enabled != settings.claude.enabled || old.dir != settings.claude.dir;
        s.settings = settings.clone();
        changed
    };
    apply(&app, claude_changed && settings.claude.enabled);
    let _ = app.emit("settings", &settings);
    Ok(detect(&dirs))
}

#[derive(Clone, Serialize)]
struct Backdrop {
    w: i32,
    h: i32,
    px: Vec<u8>,
}

#[cfg(windows)]
mod backdrop {
    use super::Backdrop;
    use std::mem::{size_of, zeroed};
    use std::ptr::null_mut;
    use windows_sys::Win32::Foundation::{HWND, RECT};
    use windows_sys::Win32::Graphics::Gdi::*;
    use windows_sys::Win32::UI::WindowsAndMessaging::*;

    const W: i32 = 64;

    pub fn exclude_from_capture(hwnd: HWND, on: bool) {
        unsafe { SetWindowDisplayAffinity(hwnd, if on { WDA_EXCLUDEFROMCAPTURE } else { WDA_NONE }) };
    }

    pub fn rect(hwnd: HWND) -> Option<RECT> {
        let mut r: RECT = unsafe { zeroed() };
        let ok = unsafe { GetWindowRect(hwnd, &mut r) } != 0 && r.right > r.left && r.bottom > r.top;
        ok.then_some(r)
    }

    fn sample(x: i32, y: i32, sw: i32, sh: i32, w: i32, h: i32) -> Option<Vec<u8>> {
        unsafe {
            let screen = GetDC(null_mut());
            let mem = CreateCompatibleDC(screen);
            let bmp = CreateCompatibleBitmap(screen, w, h);
            let old = SelectObject(mem, bmp);
            SetStretchBltMode(mem, HALFTONE);
            let copied = StretchBlt(mem, 0, 0, w, h, screen, x, y, sw, sh, SRCCOPY) != 0;
            SelectObject(mem, old);

            let mut info: BITMAPINFO = zeroed();
            info.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
            info.bmiHeader.biWidth = w;
            info.bmiHeader.biHeight = -h;
            info.bmiHeader.biPlanes = 1;
            info.bmiHeader.biBitCount = 32;
            info.bmiHeader.biCompression = BI_RGB;
            let mut px = vec![0u8; (w * h * 4) as usize];
            let rows = GetDIBits(mem, bmp, 0, h as u32, px.as_mut_ptr().cast(), &mut info, DIB_RGB_COLORS);

            DeleteObject(bmp);
            DeleteDC(mem);
            ReleaseDC(null_mut(), screen);
            if !copied || rows == 0 {
                return None;
            }
            for p in px.chunks_exact_mut(4) {
                p.swap(0, 2);
                p[3] = 255;
            }
            Some(px)
        }
    }

    pub fn grab(r: &RECT) -> Option<Backdrop> {
        let (sw, sh) = (r.right - r.left, r.bottom - r.top);
        let h = (W * sh / sw).max(1);
        sample(r.left, r.top, sw, sh, W, h).map(|px| Backdrop { w: W, h, px })
    }

    pub fn ring(r: &RECT) -> Vec<u8> {
        const M: i32 = 6;
        let (w, h) = (r.right - r.left, r.bottom - r.top);
        [
            sample(r.left - M, r.top - M, w + 2 * M, M, 24, 1),
            sample(r.left - M, r.bottom, w + 2 * M, M, 24, 1),
            sample(r.left - M, r.top, M, h, 1, 24),
            sample(r.right, r.top, M, h, 1, 24),
        ]
        .into_iter()
        .flatten()
        .flatten()
        .collect()
    }
}

#[cfg(windows)]
fn spawn_backdrop(app: AppHandle) {
    use std::hash::{Hash, Hasher};

    const SETTLE: Duration = Duration::from_millis(50);
    const EVERY: Duration = Duration::from_secs(1);
    const MIN_GAP: Duration = Duration::from_millis(200);
    let hash = |bytes: &[u8]| {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        bytes.hash(&mut h);
        h.finish()
    };
    std::thread::spawn(move || {
        let mut last = 0u64;
        let mut last_rect = (0, 0, 0, 0);
        let mut last_ring = 0u64;
        let mut last_grab: Option<Instant> = None;
        loop {
            std::thread::sleep(Duration::from_millis(50));
            let on = app.state::<Shared>().lock().unwrap().settings.blur;
            let Some(w) = app.get_webview_window("main") else { continue };
            if !on || w.is_minimized().unwrap_or(true) {
                last = 0;
                continue;
            }
            let Ok(hwnd) = w.hwnd() else { continue };
            let hwnd = hwnd.0 as _;
            let Some(r) = backdrop::rect(hwnd) else { continue };
            let ring = hash(&backdrop::ring(&r));
            let changed = (r.left, r.top, r.right, r.bottom) != last_rect || ring != last_ring;
            let since = last_grab.map_or(Duration::MAX, |t| t.elapsed());
            if since < MIN_GAP || (!changed && since < EVERY) {
                continue;
            }
            last_rect = (r.left, r.top, r.right, r.bottom);
            last_ring = ring;
            last_grab = Some(Instant::now());

            backdrop::exclude_from_capture(hwnd, true);
            std::thread::sleep(SETTLE);
            let frame = backdrop::grab(&r);
            backdrop::exclude_from_capture(hwnd, false);
            let Some(frame) = frame else { continue };
            let h = hash(&frame.px);
            if h != last {
                last = h;
                let _ = app.emit_to("main", "backdrop", frame);
            }
        }
    });
}

fn glass_effects(blur: bool) -> Option<WindowEffectsConfig> {
    blur.then(|| {
        EffectsBuilder::new()
            .effects([Effect::HudWindow])
            .state(EffectState::Active)
            .radius(16.0)
            .build()
    })
}

#[tauri::command]
async fn open_settings(app: AppHandle) -> Result<(), String> {
    if let Some(w) = app.get_webview_window("settings") {
        let _ = w.unminimize();
        return w.set_focus().map_err(|e| e.to_string());
    }
    let blur = app.state::<Shared>().lock().unwrap().settings.blur;
    let mut builder = tauri::WebviewWindowBuilder::new(&app, "settings", tauri::WebviewUrl::App("settings.html".into()))
        .title("Quotis Settings")
        .inner_size(400.0, 640.0)
        .min_inner_size(340.0, 420.0)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .always_on_top(true);
    if let Some(fx) = glass_effects(blur).filter(|_| cfg!(target_os = "macos")) {
        builder = builder.effects(fx);
    }
    builder.build().map(|_| ()).map_err(|e| e.to_string())
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![get_usage, get_settings, get_status, save_settings, open_settings])
        .on_window_event(|window, event| {
            if window.label() == "main" && matches!(event, tauri::WindowEvent::Destroyed) {
                window.app_handle().exit(0);
            }
        })
        .setup(|app| {
            let handle = app.handle().clone();
            let settings = load_settings(&handle);
            let (tx, rx) = mpsc::channel();
            app.manage(Mutex::new(State {
                dirs: Dirs::from(&settings),
                settings,
                snap: Snapshot::default(),
                gemini_files: HashMap::new(),
                claude_dirty: false,
                last_poll: None,
                claude_backoff: 0,
                watcher: notify::recommended_watcher(tx)?,
                watched: Vec::new(),
            }));
            apply(&handle, true);
            spawn_event_loop(handle.clone(), rx);
            #[cfg(windows)]
            spawn_backdrop(handle.clone());
            spawn_ticker(handle);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamps() {
        assert_eq!(parse_ts("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_ts("2000-03-01T00:00:00.123Z"), Some(951_868_800));
        assert_eq!(parse_ts("2026-09-22T09:38:12.943648+00:00"), Some(1_790_069_892));
        assert_eq!(parse_ts("garbage"), None);
    }

    #[test]
    fn codex_takes_last_rate_limits() {
        let log = r#"{"type":"event_msg","payload":{"type":"token_count","rate_limits":{"primary":{"used_percent":1.0,"window_minutes":300,"resets_at":10},"secondary":{"used_percent":2.0,"window_minutes":10080,"resets_at":20}}}}
{"type":"event_msg","payload":{"type":"token_count","info":null,"rate_limits":{"primary":{"used_percent":4.0,"window_minutes":300,"resets_at":1790096975},"secondary":{"used_percent":53.0,"window_minutes":10080,"resets_at":1790602389}}}}
{"type":"response_item","payload":{"type":"message"}}"#;
        let r = parse_codex_limits(log).unwrap();
        assert_eq!(r.five_hour.unwrap().used_percent, 4.0);
        assert_eq!(r.weekly.unwrap().resets_at, 1790602389);
    }

    #[test]
    fn claude_usage_response() {
        let body = r#"{"five_hour":{"utilization":12.0,"resets_at":"2026-09-22T14:00:00.5+00:00"},"seven_day":{"utilization":35.5,"resets_at":null},"seven_day_opus":null}"#;
        let r = parse_claude_usage(body).unwrap();
        assert_eq!(r.five_hour.as_ref().unwrap().used_percent, 12.0);
        assert_eq!(r.five_hour.unwrap().resets_at, parse_ts("2026-09-22T14:00:00Z").unwrap());
        assert_eq!(r.weekly.unwrap().resets_at, 0);
    }

    #[test]
    fn settings_fill_missing_fields_and_resolve_paths() {
        let s: Settings = serde_json::from_str(r#"{"codex":{"enabled":false},"gemini":{"dir":"~/g"}}"#).unwrap();
        assert!(s.claude.enabled && !s.codex.enabled && s.gemini.enabled);
        assert_eq!(s.claude_refresh_min, 5);
        let d = Dirs::from(&s);
        assert_eq!(d.gemini, home().join("g"));
        assert_eq!(d.codex_logs(), resolve(&s.codex, Some("CODEX_HOME"), ".codex").join("sessions"));
    }

    #[test]
    #[ignore]
    fn live_claude() {
        let r = fetch_claude(&Dirs::from(&Settings::default()).claude).ok().expect("request failed (see stderr)");
        for (name, l) in [("5h", r.five_hour), ("week", r.weekly)] {
            let l = l.expect(name);
            println!("{name}: {}% used, resets in {} min", l.used_percent, (l.resets_at - now()) / 60);
        }
    }
}
