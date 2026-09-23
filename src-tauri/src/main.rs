
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
    qwen: ProviderCfg,
    opencode: ProviderCfg,
    copilot: ProviderCfg,
    cursor: ProviderCfg,
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
            qwen: ProviderCfg::default(),
            opencode: ProviderCfg::default(),
            copilot: ProviderCfg::default(),
            cursor: ProviderCfg::default(),
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
    qwen: PathBuf,
    opencode: PathBuf,
    copilot: PathBuf,
    cursor: PathBuf,
}

fn home() -> PathBuf {
    std::env::home_dir().unwrap_or_default()
}

fn env_dir(var: &str) -> Option<PathBuf> {
    std::env::var_os(var).map(PathBuf::from)
}

fn config_base() -> PathBuf {
    if cfg!(windows) {
        env_dir("APPDATA").unwrap_or_else(|| home().join("AppData/Roaming"))
    } else if cfg!(target_os = "macos") {
        home().join("Library/Application Support")
    } else {
        env_dir("XDG_CONFIG_HOME").unwrap_or_else(|| home().join(".config"))
    }
}

fn resolve(cfg: &ProviderCfg, fallback: PathBuf) -> PathBuf {
    let dir = cfg.dir.trim();
    if let Some(rest) = dir.strip_prefix('~') {
        return home().join(rest.trim_start_matches(['/', '\\']));
    }
    if !dir.is_empty() {
        return PathBuf::from(dir);
    }
    fallback
}

impl Dirs {
    fn from(s: &Settings) -> Self {
        let copilot = if cfg!(windows) {
            env_dir("LOCALAPPDATA").unwrap_or_else(|| home().join("AppData/Local")).join("github-copilot")
        } else {
            home().join(".config/github-copilot")
        };
        Dirs {
            claude: resolve(&s.claude, env_dir("CLAUDE_CONFIG_DIR").unwrap_or_else(|| home().join(".claude"))),
            codex: resolve(&s.codex, env_dir("CODEX_HOME").unwrap_or_else(|| home().join(".codex"))),
            gemini: resolve(&s.gemini, home().join(".gemini")),
            qwen: resolve(&s.qwen, home().join(".qwen")),
            opencode: resolve(&s.opencode, home().join(".local/share/opencode")),
            copilot: resolve(&s.copilot, copilot),
            cursor: resolve(&s.cursor, config_base().join("Cursor/User/globalStorage")),
        }
    }
    fn claude_logs(&self) -> PathBuf { self.claude.join("projects") }
    fn codex_logs(&self) -> PathBuf { self.codex.join("sessions") }
    fn gemini_logs(&self) -> PathBuf { self.gemini.join("tmp") }
    fn qwen_logs(&self) -> PathBuf { self.qwen.join("tmp") }
    fn opencode_db(&self) -> PathBuf { self.opencode.join("opencode.db") }
    fn cursor_db(&self) -> PathBuf { self.cursor.join("state.vscdb") }
    fn token_logs(&self) -> [(&'static str, PathBuf); 2] {
        [("gemini", self.gemini_logs()), ("qwen", self.qwen_logs())]
    }
}

#[derive(Serialize)]
struct ToolStatus {
    path: String,
    found: bool,
}

fn detect(d: &Dirs) -> HashMap<&'static str, ToolStatus> {
    let st = |p: &Path, found: bool| ToolStatus { path: p.display().to_string(), found };
    HashMap::from([
        ("claude", st(&d.claude, claude_credentials(&d.claude).is_some())),
        ("codex", st(&d.codex, d.codex_logs().is_dir())),
        ("gemini", st(&d.gemini, d.gemini_logs().is_dir())),
        ("qwen", st(&d.qwen, d.qwen_logs().is_dir())),
        ("opencode", st(&d.opencode, d.opencode_db().is_file())),
        ("copilot", st(&d.copilot, !copilot_tokens(&d.copilot).is_empty())),
        ("cursor", st(&d.cursor, cursor_token(&d.cursor_db()).is_some())),
    ])
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
    monthly: Option<Limit>,
    tokens_5h: Option<u64>,
    tokens_24h: Option<u64>,
    cost_5h: Option<f64>,
    cost_24h: Option<f64>,
    status: Option<String>,
}

#[derive(Clone, Serialize, Default)]
struct Snapshot {
    claude: Row,
    codex: Row,
    gemini: Row,
    qwen: Row,
    opencode: Row,
    copilot: Row,
    cursor: Row,
}

impl Snapshot {
    fn row(&mut self, key: &str) -> &mut Row {
        match key {
            "claude" => &mut self.claude,
            "codex" => &mut self.codex,
            "gemini" => &mut self.gemini,
            "qwen" => &mut self.qwen,
            "opencode" => &mut self.opencode,
            "copilot" => &mut self.copilot,
            "cursor" => &mut self.cursor,
            _ => unreachable!("unknown tool {key}"),
        }
    }
}

type TokenLog = HashMap<PathBuf, Vec<(i64, u64)>>;

struct State {
    settings: Settings,
    dirs: Dirs,
    snap: Snapshot,
    token_files: HashMap<&'static str, TokenLog>,
    claude_dirty: bool,
    polls: HashMap<&'static str, Poll>,
    watcher: RecommendedWatcher,
    watched: Vec<PathBuf>,
}

#[derive(Default)]
struct Poll {
    last: Option<Instant>,
    backoff: u64,
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

fn open_ro(path: &Path) -> Option<rusqlite::Connection> {
    use rusqlite::OpenFlags;
    let c = rusqlite::Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX).ok()?;
    let _ = c.busy_timeout(Duration::from_millis(500));
    Some(c)
}

enum NetErr {
    NotSignedIn,
    Expired,
    RateLimited(Option<u64>),
    Unavailable(&'static str),
}

fn get(url: &str, headers: &[(&str, &str)]) -> Result<Value, NetErr> {
    let mut req = ureq::get(url).config().http_status_as_error(false).build().header("User-Agent", "Quotis");
    for (k, v) in headers {
        req = req.header(*k, *v);
    }
    let mut resp = req.call().map_err(|e| {
        eprintln!("GET {url} failed: {e}");
        NetErr::Unavailable("offline")
    })?;
    match resp.status().as_u16() {
        200 => {}
        429 => {
            let retry = resp.headers().get("retry-after").and_then(|v| v.to_str().ok()?.trim().parse().ok());
            return Err(NetErr::RateLimited(retry));
        }
        401 | 403 => return Err(NetErr::Expired),
        code => {
            eprintln!("GET {url}: HTTP {code}");
            return Err(NetErr::Unavailable("unavailable"));
        }
    }
    let body = resp.body_mut().read_to_string().map_err(|_| NetErr::Unavailable("bad response"))?;
    serde_json::from_str(&body).map_err(|_| NetErr::Unavailable("bad response"))
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

fn fetch_claude(root: &Path) -> Result<Row, NetErr> {
    let creds = claude_credentials(root).ok_or(NetErr::NotSignedIn)?;
    let oauth = &creds["claudeAiOauth"];
    let token = oauth.get("accessToken").and_then(Value::as_str).ok_or(NetErr::NotSignedIn)?;
    if oauth.get("expiresAt").and_then(Value::as_i64).is_some_and(|ms| ms / 1000 < now()) {
        return Err(NetErr::Expired);
    }
    let v = get(
        "https://api.anthropic.com/api/oauth/usage",
        &[("Authorization", &format!("Bearer {token}")), ("anthropic-beta", "oauth-2025-04-20")],
    )?;
    parse_claude_usage(&v).ok_or(NetErr::Unavailable("bad response"))
}

fn parse_claude_usage(v: &Value) -> Option<Row> {
    let lim = |k: &str| {
        let w = v.get(k)?;
        Some(Limit {
            used_percent: w.get("utilization")?.as_f64()?,
            resets_at: w.get("resets_at").and_then(Value::as_str).and_then(parse_ts).unwrap_or(0),
        })
    };
    Some(Row { five_hour: lim("five_hour"), weekly: lim("seven_day"), ..Default::default() })
}

fn copilot_tokens(dir: &Path) -> Vec<String> {
    ["apps.json", "hosts.json"]
        .iter()
        .filter_map(|f| std::fs::read_to_string(dir.join(f)).ok())
        .filter_map(|t| serde_json::from_str::<HashMap<String, Value>>(&t).ok())
        .flat_map(|m| m.into_values())
        .filter_map(|v| v.get("oauth_token")?.as_str().map(String::from))
        .collect()
}

fn fetch_copilot(dir: &Path) -> Result<Row, NetErr> {
    let tokens = copilot_tokens(dir);
    if tokens.is_empty() {
        return Err(NetErr::NotSignedIn);
    }
    let mut err = NetErr::Expired;
    for t in tokens {
        match get("https://api.github.com/copilot_internal/user", &[("Authorization", &format!("token {t}")), ("Accept", "application/json")]) {
            Ok(v) => return parse_copilot(&v).ok_or(NetErr::Unavailable("bad response")),
            Err(NetErr::Expired) => continue,
            Err(e) => err = e,
        }
    }
    Err(err)
}

fn parse_copilot(v: &Value) -> Option<Row> {
    let p = v.pointer("/quota_snapshots/premium_interactions")?;
    if p.get("unlimited").and_then(Value::as_bool) == Some(true) {
        return Some(Row { status: Some("unlimited premium requests".into()), ..Default::default() });
    }
    let left = p.get("percent_remaining")?.as_f64()?.clamp(0.0, 100.0);
    let resets_at = v
        .get("quota_reset_date_utc")
        .or_else(|| v.get("quota_reset_date"))
        .and_then(Value::as_str)
        .and_then(|s| parse_ts(s).or_else(|| parse_ts(&format!("{s}T00:00:00Z"))))
        .unwrap_or(0);
    Some(Row { monthly: Some(Limit { used_percent: 100.0 - left, resets_at }), ..Default::default() })
}

fn cursor_token(db: &Path) -> Option<String> {
    open_ro(db)?
        .query_row("SELECT value FROM ItemTable WHERE key = 'cursorAuth/accessToken'", [], |r| r.get::<_, String>(0))
        .ok()
        .filter(|t| !t.is_empty())
}

fn fetch_cursor(db: &Path) -> Result<Row, NetErr> {
    let token = cursor_token(db).ok_or(NetErr::NotSignedIn)?;
    let payload = token.split('.').nth(1).ok_or(NetErr::NotSignedIn)?;
    let claims: Value = serde_json::from_slice(&base64url(payload).ok_or(NetErr::NotSignedIn)?).map_err(|_| NetErr::NotSignedIn)?;
    let user = claims.get("sub").and_then(Value::as_str).and_then(|s| s.rsplit('|').next()).ok_or(NetErr::NotSignedIn)?;
    let v = get("https://cursor.com/api/usage-summary", &[("Cookie", &format!("WorkosCursorSessionToken={user}%3A%3A{token}"))])?;
    parse_cursor(&v).ok_or(NetErr::Unavailable("bad response"))
}

fn parse_cursor(v: &Value) -> Option<Row> {
    if v.get("isUnlimited").and_then(Value::as_bool) == Some(true) {
        return Some(Row { status: Some("unlimited plan".into()), ..Default::default() });
    }
    let plan = v.pointer("/individualUsage/plan")?;
    let resets_at = v.get("billingCycleEnd").and_then(Value::as_str).and_then(parse_ts).unwrap_or(0);
    if plan.get("limit").and_then(Value::as_f64) == Some(0.0) {
        let tier = v.get("membershipType").and_then(Value::as_str).unwrap_or("current");
        return Some(Row { status: Some(format!("{tier} plan, no included usage")), ..Default::default() });
    }
    let used = plan.get("totalPercentUsed")?.as_f64()?.clamp(0.0, 100.0);
    Some(Row { monthly: Some(Limit { used_percent: used, resets_at }), ..Default::default() })
}

fn base64url(s: &str) -> Option<Vec<u8>> {
    let val = |c: u8| match c {
        b'A'..=b'Z' => Some(c - b'A'),
        b'a'..=b'z' => Some(c - b'a' + 26),
        b'0'..=b'9' => Some(c - b'0' + 52),
        b'-' | b'+' => Some(62),
        b'_' | b'/' => Some(63),
        _ => None,
    };
    let (mut out, mut acc, mut bits) = (Vec::new(), 0u32, 0);
    for c in s.bytes().filter(|&c| c != b'=') {
        acc = (acc << 6) | val(c)? as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Some(out)
}

const ONLINE: [&str; 3] = ["claude", "copilot", "cursor"];

fn enabled(s: &Settings, key: &str) -> bool {
    match key {
        "claude" => s.claude.enabled,
        "codex" => s.codex.enabled,
        "gemini" => s.gemini.enabled,
        "qwen" => s.qwen.enabled,
        "opencode" => s.opencode.enabled,
        "copilot" => s.copilot.enabled,
        "cursor" => s.cursor.enabled,
        _ => false,
    }
}

fn poll(app: &AppHandle, key: &'static str) {
    let dirs = {
        let state = app.state::<Shared>();
        let s = state.lock().unwrap();
        if !enabled(&s.settings, key) {
            return;
        }
        s.dirs.clone()
    };
    let (result, expired) = match key {
        "claude" => (fetch_claude(&dirs.claude), "login expired, open Claude Code"),
        "copilot" => (fetch_copilot(&dirs.copilot), "login expired, sign in to Copilot again"),
        _ => (fetch_cursor(&dirs.cursor_db()), "login expired, open Cursor"),
    };
    {
        let state = app.state::<Shared>();
        let mut guard = state.lock().unwrap();
        let s = &mut *guard;
        s.polls.entry(key).or_default().last = Some(Instant::now());
        if key == "claude" {
            s.claude_dirty = false;
        }
        let same_dirs = s.dirs.claude == dirs.claude && s.dirs.copilot == dirs.copilot && s.dirs.cursor == dirs.cursor;
        if enabled(&s.settings, key) && same_dirs {
            let row = s.snap.row(key);
            let has_data = row.five_hour.is_some() || row.weekly.is_some() || row.monthly.is_some();
            let problem = |msg: &str| Row { connected: true, status: Some(msg.into()), ..Default::default() };
            let p = s.polls.get_mut(key).unwrap();
            match result {
                Ok(mut r) => {
                    r.connected = true;
                    p.backoff = 0;
                    *s.snap.row(key) = r;
                }
                Err(NetErr::NotSignedIn) => *s.snap.row(key) = Row::default(),
                Err(NetErr::Expired) => *s.snap.row(key) = problem(expired),
                Err(NetErr::RateLimited(retry)) => {
                    p.backoff = retry.unwrap_or((p.backoff * 2).max(300)).min(3600);
                    if !has_data {
                        *s.snap.row(key) = problem("rate limited, retrying later");
                    }
                }
                Err(NetErr::Unavailable(msg)) => {
                    if !has_data {
                        *s.snap.row(key) = problem(msg);
                    }
                }
            }
        }
    }
    emit(app);
}

fn codex_row(logs: &Path) -> Row {
    let mut files: Vec<_> = Vec::new();
    walk(logs, &mut files);
    files.retain(|p| is_jsonl(p));
    files.sort_by_key(|p| std::cmp::Reverse(p.metadata().and_then(|m| m.modified()).unwrap_or(UNIX_EPOCH)));
    let mut row = files
        .iter()
        .take(20)
        .find_map(|p| parse_codex_limits(&std::fs::read_to_string(p).ok()?))
        .unwrap_or_else(|| Row { status: Some("no limit data yet".into()), ..Default::default() });
    row.connected = true;
    row
}

fn parse_codex_limits(text: &str) -> Option<Row> {
    text.lines().rev().filter(|l| l.contains("\"rate_limits\"")).find_map(|line| {
        let v: Value = serde_json::from_str(line).ok()?;
        let rl = v.pointer("/payload/rate_limits")?;
        let lim = |k: &str| {
            let w = rl.get(k)?;
            Some(Limit { used_percent: w.get("used_percent")?.as_f64()?, resets_at: w.get("resets_at")?.as_i64()? })
        };
        let row = Row { five_hour: lim("primary"), weekly: lim("secondary"), ..Default::default() };
        (row.five_hour.is_some() || row.weekly.is_some()).then_some(row)
    })
}

fn token_entries(path: &Path) -> Vec<(i64, u64)> {
    let Ok(text) = std::fs::read_to_string(path) else { return Vec::new() };
    text.lines()
        .filter(|l| l.contains("\"tokens\""))
        .filter_map(|l| {
            let v: Value = serde_json::from_str(l).ok()?;
            Some((parse_ts(v.get("timestamp")?.as_str()?)?, v.pointer("/tokens/total")?.as_u64()?))
        })
        .collect()
}

fn scan_tokens(logs: &Path) -> TokenLog {
    let mut files = Vec::new();
    walk(logs, &mut files);
    let cutoff = SystemTime::now() - Duration::from_secs(24 * 3600);
    files
        .into_iter()
        .filter(|p| is_jsonl(p) && p.metadata().and_then(|m| m.modified()).is_ok_and(|t| t > cutoff))
        .map(|p| {
            let e = token_entries(&p);
            (p, e)
        })
        .collect()
}

fn opencode_row(db: &Path) -> Row {
    let Some(c) = open_ro(db) else {
        return Row { connected: true, status: Some("can't read opencode.db".into()), ..Default::default() };
    };
    let since = |secs: i64| {
        c.query_row(
            "SELECT COALESCE(SUM(COALESCE(json_extract(data,'$.tokens.input'),0) + COALESCE(json_extract(data,'$.tokens.output'),0)
                     + COALESCE(json_extract(data,'$.tokens.reasoning'),0)),0),
                    COALESCE(SUM(json_extract(data,'$.cost')),0)
             FROM message WHERE time_created >= ?1 AND json_extract(data,'$.role') = 'assistant'",
            [(now() - secs) * 1000],
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, f64>(1)?)),
        )
        .unwrap_or((0, 0.0))
    };
    let ((t5, c5), (t24, c24)) = (since(5 * 3600), since(24 * 3600));
    Row {
        connected: true,
        tokens_5h: Some(t5.max(0) as u64),
        tokens_24h: Some(t24.max(0) as u64),
        cost_5h: Some(c5),
        cost_24h: Some(c24),
        ..Default::default()
    }
}

fn emit(app: &AppHandle) {
    let snap = {
        let state = app.state::<Shared>();
        let mut guard = state.lock().unwrap();
        let s = &mut *guard;
        let t = now();
        for (key, files) in &s.token_files {
            let row = s.snap.row(key);
            if row.connected {
                let sum = |secs: i64| files.values().flatten().filter(|(ts, _)| t - ts < secs).map(|(_, n)| n).sum::<u64>();
                row.tokens_5h = Some(sum(5 * 3600));
                row.tokens_24h = Some(sum(24 * 3600));
            }
        }
        if s.snap.opencode.connected {
            s.snap.opencode = opencode_row(&s.dirs.opencode_db());
        }
        s.snap.clone()
    };
    let _ = app.emit("usage", snap);
}

fn apply(app: &AppHandle, poll_now: &[&'static str]) {
    {
        let state = app.state::<Shared>();
        let mut guard = state.lock().unwrap();
        let s = &mut *guard;
        s.dirs = Dirs::from(&s.settings);
        for p in s.watched.drain(..) {
            let _ = s.watcher.unwatch(&p);
        }
        let codex_on = s.settings.codex.enabled && s.dirs.codex_logs().is_dir();
        let opencode_on = s.settings.opencode.enabled && s.dirs.opencode_db().is_file();
        let mut targets = vec![
            (s.settings.claude.enabled, s.dirs.claude_logs()),
            (codex_on, s.dirs.codex_logs()),
            (opencode_on, s.dirs.opencode.clone()),
        ];
        s.token_files.clear();
        for (key, logs) in s.dirs.token_logs() {
            let found = enabled(&s.settings, key) && logs.is_dir();
            *s.snap.row(key) = Row { connected: found, ..Default::default() };
            if found {
                s.token_files.insert(key, scan_tokens(&logs));
                targets.push((true, logs));
            }
        }
        for (on, dir) in targets {
            if !on || !dir.is_dir() {
                continue;
            }
            match s.watcher.watch(&dir, RecursiveMode::Recursive) {
                Ok(()) => s.watched.push(dir),
                Err(e) => eprintln!("watch {}: {e}", dir.display()),
            }
        }

        for key in ONLINE {
            if poll_now.contains(&key) || !enabled(&s.settings, key) {
                *s.snap.row(key) = Row::default();
            }
        }
        s.snap.codex = if codex_on { codex_row(&s.dirs.codex_logs()) } else { Row::default() };
        s.snap.opencode = if opencode_on { opencode_row(&s.dirs.opencode_db()) } else { Row::default() };
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
    for &key in poll_now {
        let app = app.clone();
        std::thread::spawn(move || poll(&app, key));
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
            let touched = |dir: &Path| changed.iter().any(|p| p.starts_with(dir));
            let codex = touched(&d.codex_logs()).then(|| codex_row(&d.codex_logs()));
            let opencode = touched(&d.opencode).then(|| opencode_row(&d.opencode_db()));
            let mut tokens = Vec::new();
            for (key, logs) in d.token_logs() {
                for p in changed.iter().filter(|p| p.starts_with(&logs) && is_jsonl(p)) {
                    tokens.push((key, p.clone(), token_entries(p)));
                }
            }

            let state = app.state::<Shared>();
            let mut s = state.lock().unwrap();
            if touched(&d.claude_logs()) {
                s.claude_dirty = true;
            }
            if let Some(row) = codex.filter(|_| s.snap.codex.connected) {
                s.snap.codex = row;
            }
            if let Some(row) = opencode.filter(|_| s.snap.opencode.connected) {
                s.snap.opencode = row;
            }
            for (key, p, e) in tokens {
                if let Some(files) = s.token_files.get_mut(key) {
                    files.insert(p, e);
                }
            }
            drop(s);
            emit(&app);
        }
    });
}

fn spawn_ticker(app: AppHandle) {
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(15));
        let due: Vec<&'static str> = {
            let state = app.state::<Shared>();
            let s = state.lock().unwrap();
            let base = s.settings.claude_refresh_min.clamp(1, 60) * 60;
            ONLINE
                .into_iter()
                .filter(|&key| enabled(&s.settings, key))
                .filter(|&key| {
                    let p = s.polls.get(key);
                    let backoff = p.map_or(0, |p| p.backoff);
                    p.and_then(|p| p.last).is_none_or(|t| {
                        let secs = t.elapsed().as_secs();
                        secs >= base.max(backoff) || (key == "claude" && s.claude_dirty && secs >= 120.max(backoff))
                    })
                })
                .collect()
        };
        for key in &due {
            poll(&app, key);
        }
        if due.is_empty() {
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
async fn get_status(app: AppHandle) -> HashMap<&'static str, ToolStatus> {
    let dirs = app.state::<Shared>().lock().unwrap().dirs.clone();
    detect(&dirs)
}

#[tauri::command]
async fn save_settings(app: AppHandle, mut settings: Settings) -> Result<HashMap<&'static str, ToolStatus>, String> {
    settings.claude_refresh_min = settings.claude_refresh_min.clamp(1, 60);
    settings.glass_opacity = settings.glass_opacity.min(100);
    let path = settings_path(&app).ok_or("no config folder")?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())?;
    let dirs = Dirs::from(&settings);
    let repoll: Vec<&'static str> = {
        let state = app.state::<Shared>();
        let mut s = state.lock().unwrap();
        let cfg = |st: &Settings, key: &str| match key {
            "claude" => st.claude.clone(),
            "copilot" => st.copilot.clone(),
            _ => st.cursor.clone(),
        };
        let changed = ONLINE
            .into_iter()
            .filter(|&k| {
                let (a, b) = (cfg(&s.settings, k), cfg(&settings, k));
                b.enabled && (a.enabled != b.enabled || a.dir != b.dir)
            })
            .collect();
        s.settings = settings.clone();
        changed
    };
    apply(&app, &repoll);
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
                token_files: HashMap::new(),
                claude_dirty: false,
                polls: HashMap::new(),
                watcher: notify::recommended_watcher(tx)?,
                watched: Vec::new(),
            }));
            apply(&handle, &ONLINE);
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
{"type":"response_item","payload":{"type":"message"}}
{"type":"event_msg","payload":{"type":"token_count","rate_limits":null}}"#;
        let r = parse_codex_limits(log).unwrap();
        assert_eq!(r.five_hour.unwrap().used_percent, 4.0);
        assert_eq!(r.weekly.unwrap().resets_at, 1790602389);
        assert!(parse_codex_limits(r#"{"payload":{"rate_limits":null}}"#).is_none());
    }

    #[test]
    fn claude_usage_response() {
        let v = serde_json::json!({"five_hour":{"utilization":12.0,"resets_at":"2026-09-22T14:00:00.5+00:00"},"seven_day":{"utilization":35.5,"resets_at":null},"seven_day_opus":null});
        let r = parse_claude_usage(&v).unwrap();
        assert_eq!(r.five_hour.as_ref().unwrap().used_percent, 12.0);
        assert_eq!(r.five_hour.unwrap().resets_at, parse_ts("2026-09-22T14:00:00Z").unwrap());
        assert_eq!(r.weekly.unwrap().resets_at, 0);
    }

    #[test]
    fn copilot_premium_requests() {
        let v = serde_json::json!({"quota_reset_date":"2026-10-01","quota_reset_date_utc":"2026-10-01T00:00:00.000Z",
            "quota_snapshots":{"premium_interactions":{"percent_remaining":37.5,"unlimited":false,"entitlement":300}}});
        let m = parse_copilot(&v).unwrap().monthly.unwrap();
        assert_eq!(m.used_percent, 62.5);
        assert_eq!(m.resets_at, parse_ts("2026-10-01T00:00:00Z").unwrap());
        let over = serde_json::json!({"quota_reset_date":"2026-10-01","quota_snapshots":{"premium_interactions":{"percent_remaining":-1.0,"unlimited":false}}});
        let m = parse_copilot(&over).unwrap().monthly.unwrap();
        assert_eq!((m.used_percent, m.resets_at), (100.0, parse_ts("2026-10-01T00:00:00Z").unwrap()));
        let unl = serde_json::json!({"quota_snapshots":{"premium_interactions":{"unlimited":true}}});
        assert!(parse_copilot(&unl).unwrap().status.is_some());
    }

    #[test]
    fn cursor_usage_summary() {
        let v = serde_json::json!({"billingCycleEnd":"2026-10-01T09:23:29.338Z","membershipType":"pro","isUnlimited":false,
            "individualUsage":{"plan":{"used":1200,"limit":2000,"totalPercentUsed":60.0}}});
        let m = parse_cursor(&v).unwrap().monthly.unwrap();
        assert_eq!((m.used_percent, m.resets_at), (60.0, parse_ts("2026-10-01T09:23:29Z").unwrap()));
        let free = serde_json::json!({"membershipType":"free","isUnlimited":false,"individualUsage":{"plan":{"limit":0,"totalPercentUsed":0}}});
        assert_eq!(parse_cursor(&free).unwrap().status.as_deref(), Some("free plan, no included usage"));
    }

    #[test]
    fn jwt_payload_decodes() {
        let bytes = base64url("eyJzdWIiOiJhdXRoMHx1c2VyXzEyMyJ9").unwrap();
        let v: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(v["sub"], "auth0|user_123");
    }

    #[test]
    fn opencode_sums_recent_assistant_messages() {
        let dir = std::env::temp_dir().join(format!("quotis-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = dir.join("opencode.db");
        let _ = std::fs::remove_file(&db);
        let c = rusqlite::Connection::open(&db).unwrap();
        c.execute_batch("CREATE TABLE message (id TEXT, session_id TEXT, time_created INTEGER, time_updated INTEGER, data TEXT)").unwrap();
        let ms = |ago: i64| (now() - ago) * 1000;
        let add = |t: i64, data: &str| c.execute("INSERT INTO message VALUES ('m','s',?1,?1,?2)", rusqlite::params![t, data]).unwrap();
        add(ms(60), r#"{"role":"assistant","cost":0.5,"tokens":{"input":100,"output":50,"reasoning":10,"cache":{"read":9999}}}"#);
        add(ms(6 * 3600), r#"{"role":"assistant","cost":1.25,"tokens":{"input":1000,"output":0}}"#);
        add(ms(30 * 3600), r#"{"role":"assistant","cost":9.0,"tokens":{"input":5000,"output":5000}}"#);
        add(ms(60), r#"{"role":"user"}"#);
        drop(c);
        let r = opencode_row(&db);
        assert_eq!((r.tokens_5h, r.tokens_24h), (Some(160), Some(1160)));
        assert!((r.cost_5h.unwrap() - 0.5).abs() < 1e-9 && (r.cost_24h.unwrap() - 1.75).abs() < 1e-9);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn settings_fill_missing_fields_and_resolve_paths() {
        let s: Settings = serde_json::from_str(r#"{"codex":{"enabled":false},"gemini":{"dir":"~/g"}}"#).unwrap();
        assert!(s.claude.enabled && !s.codex.enabled && s.gemini.enabled && s.opencode.enabled && s.cursor.enabled);
        assert_eq!(s.claude_refresh_min, 5);
        let d = Dirs::from(&s);
        assert_eq!(d.gemini, home().join("g"));
        assert_eq!(d.qwen_logs(), home().join(".qwen").join("tmp"));
        assert!(d.cursor_db().ends_with("Cursor/User/globalStorage/state.vscdb"));
    }

    #[test]
    #[ignore]
    fn live_online_sources() {
        let d = Dirs::from(&Settings::default());
        for (name, r) in [("claude", fetch_claude(&d.claude)), ("copilot", fetch_copilot(&d.copilot)), ("cursor", fetch_cursor(&d.cursor_db()))] {
            match r {
                Ok(r) => {
                    let show = |l: Option<Limit>| l.map(|l| format!("{:.0}% used, resets in {}h", l.used_percent, (l.resets_at - now()) / 3600));
                    println!("{name}: 5h={:?} week={:?} month={:?} status={:?}", show(r.five_hour), show(r.weekly), show(r.monthly), r.status);
                }
                Err(_) => println!("{name}: not available"),
            }
        }
        println!("opencode: {:?}", { let r = opencode_row(&d.opencode_db()); (r.tokens_5h, r.tokens_24h, r.cost_5h, r.cost_24h) });
    }
}
