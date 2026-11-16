//! quotis-cli: a read-only window onto Quotis usage data for people and coding agents.
//!
//! By default it only reads the snapshot file the desktop app keeps up to date, so running
//! it costs no network request. `--live` collects once itself. It never writes anything,
//! takes no command, path or URL to act on, and never prints a credential. (With `--live`
//! on macOS, reading the Claude Code login asks the system `security` tool for it.)

use quotis_core::cache::{self, CacheError};
use quotis_core::*;
use serde_json::{json, Value};
use std::path::Path;

const USAGE: &str = "quotis-cli - read-only AI usage snapshots from Quotis

USAGE:
    quotis-cli <COMMAND> [OPTIONS]

COMMANDS:
    status                    Summary of every connected provider
    snapshot                  The full normalized snapshot
    providers                 Every known provider, where Quotis looks for it, what it contacts
    diagnose --provider <ID>  Why one provider is or is not reporting
    export [--format json]    A sanitized snapshot to share or analyse offline

OPTIONS:
    --json           Machine-readable output (always on for snapshot and export)
    --live           Collect now (status, snapshot, diagnose, export); waits at most 60s for collectors
    --offline        With --live: read local files only, make no network request
    --max-age <SEC>  Treat a desktop snapshot older than this as stale (default 180)
    -h, --help       Show this help
    -V, --version    Show the version

EXIT CODES:
    0 ok   2 bad usage   3 no snapshot (start Quotis or pass --live)
    4 snapshot is stale (output is still printed)   5 snapshot from an incompatible version
    6 snapshot file is malformed   7 unknown provider";

const EXIT_USAGE: i32 = 2;
const EXIT_NO_SNAPSHOT: i32 = 3;
const EXIT_STALE: i32 = 4;
const EXIT_INCOMPATIBLE: i32 = 5;
const EXIT_MALFORMED: i32 = 6;
const EXIT_UNKNOWN_PROVIDER: i32 = 7;

struct Args {
    command: String,
    json: bool,
    live: bool,
    offline: bool,
    max_age: i64,
    provider: Option<String>,
}

fn parse_args(argv: &[String]) -> Result<Args, String> {
    let mut a = Args { command: String::new(), json: false, live: false, offline: false, max_age: 180, provider: None };
    let mut format_given = false;
    let mut it = argv.iter();
    while let Some(arg) = it.next() {
        let mut value = |name: &str| it.next().cloned().ok_or(format!("{name} needs a value"));
        match arg.as_str() {
            "--json" => a.json = true,
            "--live" => a.live = true,
            "--offline" => a.offline = true,
            "--max-age" => {
                a.max_age = value("--max-age")?.parse().ok().filter(|n| (0..=86_400 * 365).contains(n)).ok_or("--max-age must be a number of seconds")?
            }
            "--provider" => a.provider = Some(value("--provider")?),
            "--format" => {
                format_given = true;
                if value("--format")? != "json" {
                    return Err("the only export format is json".into());
                }
            }
            s if s.starts_with('-') => return Err(format!("unknown option {s}")),
            s if a.command.is_empty() => a.command = s.to_string(),
            s => return Err(format!("unexpected argument {s}")),
        }
    }
    if !["status", "snapshot", "providers", "diagnose", "export"].contains(&a.command.as_str()) {
        return Err(if a.command.is_empty() { "missing command".into() } else { format!("unknown command {}", a.command) });
    }
    if a.command == "diagnose" && a.provider.is_none() {
        return Err("diagnose needs --provider <id>".into());
    }
    if a.offline && !a.live {
        return Err("--offline only applies with --live".into());
    }
    if a.command == "providers" && a.live {
        return Err("--live does not apply to providers; that command only discovers local sources".into());
    }
    if a.provider.is_some() && a.command != "diagnose" {
        return Err("--provider only applies to diagnose".into());
    }
    if format_given && a.command != "export" {
        return Err("--format only applies to export".into());
    }
    a.json |= matches!(a.command.as_str(), "snapshot" | "export");
    Ok(a)
}

/// Where the data came from and how old it is.
struct Loaded {
    snap: Snapshot,
    source: &'static str,
    age: i64,
    stale: bool,
    statuses: Vec<ProviderStatus>,
}

fn load(a: &Args, dir: &Path, now: i64) -> Result<Loaded, (i32, &'static str, String)> {
    if a.live {
        let mut engine = Engine::new(Settings::load(dir));
        let snap = engine.collect_once(!a.offline);
        return Ok(Loaded { snap, source: if a.offline { "live_local_only" } else { "live" }, age: 0, stale: false, statuses: engine.statuses() });
    }
    match cache::read(dir) {
        Ok(mut snap) => {
            // The file says what was fresh when it was written. Judge it again for now:
            // a window may have reset, or the numbers aged, since then.
            snap.refresh(now);
            let age = now - snap.generated_at;
            // A snapshot dated in the future means the clock changed; its age is unknown.
            let stale = !(-300..=a.max_age).contains(&age);
            Ok(Loaded { snap, source: "desktop_cache", age: age.max(0), stale, statuses: Engine::new(Settings::load(dir)).statuses() })
        }
        Err(CacheError::Missing) => {
            Err((EXIT_NO_SNAPSHOT, "no_snapshot", "No snapshot found. Start the Quotis desktop app, or pass --live to collect now.".into()))
        }
        Err(CacheError::Malformed) => {
            Err((EXIT_MALFORMED, "malformed_snapshot", "The snapshot file is not valid. Restart Quotis to rewrite it, or pass --live.".into()))
        }
        Err(CacheError::Incompatible(v)) => Err((
            EXIT_INCOMPATIBLE,
            "incompatible_schema",
            format!(
                "The snapshot uses schema version {v}; this quotis-cli understands version {SCHEMA_VERSION}. Update Quotis and quotis-cli to the same release."
            ),
        )),
    }
}

fn envelope(command: &str, l: &Loaded, data: Value) -> Value {
    json!({
        "schema_version": SCHEMA_VERSION,
        "command": command,
        "source": l.source,
        "generated_at": l.snap.generated_at,
        "age_seconds": l.age,
        "stale": l.stale,
        "data": data,
    })
}

fn until(ts: i64, now: i64) -> String {
    let s = ts - now;
    let (d, h, m) = (s / 86_400, (s % 86_400) / 3600, (s % 3600) / 60);
    if d > 0 {
        format!("{d}d {h}h")
    } else if h > 0 {
        format!("{h}h {m}m")
    } else {
        format!("{}m", m.max(1))
    }
}

fn amount(v: f64, unit: Unit) -> String {
    match unit {
        Unit::Percent => format!("{v:.0}%"),
        Unit::Usd => format!("${v:.2}"),
        Unit::Cny => format!("CNY {v:.2}"),
        Unit::Tokens if v >= 1e6 => format!("{:.1}M tokens", v / 1e6),
        Unit::Tokens if v >= 1e3 => format!("{:.1}K tokens", v / 1e3),
        Unit::Tokens => format!("{v:.0} tokens"),
        Unit::Requests => format!("{v:.0} requests"),
    }
}

fn describe(m: &Metric, now: i64) -> String {
    let mut s = match (m.remaining, m.used) {
        (Some(left), _) if m.category == Category::ProviderQuota => format!("{} {} left", m.label, amount(left, m.unit)),
        (_, Some(used)) => format!("{} {} used", m.label, amount(used, m.unit)),
        _ => format!("{} unavailable", m.label),
    };
    if m.category == Category::EstimatedCost {
        s.push_str(" (estimate)");
    }
    match (m.freshness, m.resets_at) {
        (Freshness::Expired, _) => s.push_str(" [window reset since this was read; awaiting refresh]"),
        (f, Some(r)) if r > now => s.push_str(&format!(" (resets in {}{})", until(r, now), if f == Freshness::Stale { ", stale" } else { "" })),
        (Freshness::Stale, _) => s.push_str(" [stale]"),
        _ => {}
    }
    if let Some(qualification) = &m.qualification {
        s.push_str(&format!(" [{qualification}]"));
    }
    s
}

fn status_word(p: &ProviderSnapshot) -> &'static str {
    match p.status {
        Status::Connected if p.error.is_some() => "degraded",
        Status::Connected => "connected",
        Status::NotDetected => "not detected",
        Status::Disabled => "off",
        Status::Error => "error",
    }
}

fn status_text(l: &Loaded, now: i64) -> String {
    let mut out = String::new();
    let shown: Vec<_> = l.snap.providers.iter().filter(|p| !matches!(p.status, Status::Disabled | Status::NotDetected)).collect();
    if shown.is_empty() {
        out.push_str("No providers connected. Run `quotis-cli providers` to see what Quotis looks for.\n");
    }
    for p in shown {
        let mut parts: Vec<String> = p.metrics.iter().map(|m| describe(m, now)).collect();
        parts.extend(p.notes.iter().cloned());
        if let Some(e) = &p.error {
            parts.push(format!("{} ({:?})", e.message, e.kind));
        }
        out.push_str(&format!("{:<15} {:<10} {}\n", p.name, status_word(p), parts.join(" · ")));
    }
    if l.stale {
        out.push_str(&format!("\nWarning: this snapshot is {}s old. Is the Quotis desktop app running? Pass --live to collect now.\n", l.age));
    }
    out
}

fn advice(p: &ProviderSnapshot, st: &ProviderStatus) -> String {
    let kind = p.error.as_ref().map(|e| e.kind);
    match (p.status, kind) {
        (Status::Disabled, _) if st.info.key_env.is_some() => {
            format!("Off by default. Set {} in your environment and turn {} on in Quotis Settings.", st.info.key_env.unwrap_or(""), p.name)
        }
        (Status::Disabled, _) => format!("{} is turned off in Quotis Settings.", p.name),
        (_, Some(ErrorKind::AuthExpired)) => {
            format!("The stored {} login has expired. Open {} once so it refreshes its login; Quotis never refreshes logins itself.", p.name, p.name)
        }
        (_, Some(ErrorKind::RateLimited)) => "The provider is rate limiting usage checks. Quotis is backing off and will retry on its own.".into(),
        (_, Some(ErrorKind::Offline)) => {
            "The provider could not be reached. Check the network connection; the last known numbers are kept and marked stale.".into()
        }
        (_, Some(ErrorKind::PermissionDenied)) => {
            "The operating system denied access to this tool's files or login. Grant access, or turn the provider off.".into()
        }
        (_, Some(ErrorKind::BadResponse)) => "The provider answered in an unexpected format. Its endpoint may have changed; update Quotis.".into(),
        (_, Some(ErrorKind::Unavailable)) => "The provider or its local data is temporarily unavailable. Quotis will retry.".into(),
        (_, Some(ErrorKind::Internal)) => "Quotis hit an internal error in this provider. Please report it.".into(),
        (Status::NotDetected, _) if st.info.key_env.is_some() => format!("{} is not set in the environment Quotis runs in.", st.info.key_env.unwrap_or("")),
        (Status::NotDetected, _) => {
            format!("No {} login or data was found in {}. Install or sign in to {}, or set its folder in Quotis Settings.", p.name, st.path, p.name)
        }
        _ if p.metrics.iter().any(|m| m.freshness != Freshness::Fresh) => {
            "Connected, but some numbers are stale or their window has reset since they were read. They refresh on the next successful check.".into()
        }
        _ if p.metrics.iter().any(|m| m.qualification.is_some()) => {
            "Collection is partial or qualified; a reported lower bound is not the complete usage total. Check the metric caveats.".into()
        }
        _ => "Connected and reporting.".into(),
    }
}

/// Runs one command. Returns the process exit code and what to print.
fn run(argv: &[String], dir: &Path, now: i64) -> (i32, String) {
    if argv.iter().any(|a| a == "-h" || a == "--help") || argv.is_empty() {
        return (if argv.is_empty() { EXIT_USAGE } else { 0 }, format!("{USAGE}\n"));
    }
    if argv.iter().any(|a| a == "-V" || a == "--version") {
        return (0, format!("quotis-cli {} (schema {SCHEMA_VERSION})\n", env!("CARGO_PKG_VERSION")));
    }
    let wants_json = argv.iter().any(|a| a == "--json");
    let fail = |code: i32, id: &str, message: String, json: bool| {
        let text = if json {
            format!("{}\n", json!({"schema_version": SCHEMA_VERSION, "error": {"code": id, "message": message}}))
        } else {
            format!("error: {message}\n")
        };
        (code, text)
    };
    let a = match parse_args(argv) {
        Ok(a) => a,
        Err(msg) => return fail(EXIT_USAGE, "usage", format!("{msg}. Run quotis-cli --help."), wants_json),
    };

    if a.command == "providers" {
        let mut engine = Engine::new(Settings::load(dir));
        engine.probe_now();
        let statuses = engine.statuses();
        if a.json {
            return (0, format!("{}\n", json!({"schema_version": SCHEMA_VERSION, "command": "providers", "data": statuses})));
        }
        let mut out = String::new();
        for s in statuses {
            let state = if !s.enabled {
                "off"
            } else if s.found {
                "found"
            } else {
                "not found"
            };
            let contact = s
                .info
                .endpoint
                .map_or("local files only".to_string(), |h| format!("{h} ({})", if s.info.documented { "documented API" } else { "undocumented endpoint" }));
            let place = s.info.key_env.map_or(s.path.clone(), |k| format!("${k}"));
            out.push_str(&format!("{:<11} {:<10} {:<38} {}\n", s.info.id, state, contact, place));
        }
        return (0, out);
    }

    let l = match load(&a, dir, now) {
        Ok(l) => l,
        Err((code, id, msg)) => return fail(code, id, msg, a.json),
    };
    let code = if l.stale { EXIT_STALE } else { 0 };
    let print = |v: Value| format!("{}\n", serde_json::to_string_pretty(&v).unwrap_or_default());

    match a.command.as_str() {
        "snapshot" => (code, print(envelope("snapshot", &l, json!(l.snap)))),
        "export" => {
            // An export is meant to be pasted elsewhere: drop plan names and anything not needed to reason about usage.
            let mut snap = l.snap.clone();
            for p in &mut snap.providers {
                p.account = None;
            }
            (code, print(json!({"quotis_export": 1, "exported_at": now, "stale": l.stale, "source": l.source, "snapshot": snap})))
        }
        "status" if a.json => {
            let rows: Vec<Value> = l
                .snap
                .providers
                .iter()
                .map(|p| json!({"id": p.id, "name": p.name, "status": p.status, "error": p.error, "metrics": p.metrics, "notes": p.notes}))
                .collect();
            (code, print(envelope("status", &l, json!(rows))))
        }
        "status" => (code, status_text(&l, now)),
        _ => {
            let id = a.provider.as_deref().unwrap_or("");
            // Reuse the bounded collection's detection. A second synchronous probe here
            // would bypass the live deadline and make cached diagnosis read logins.
            let statuses = &l.statuses;
            let (Some(p), Some(st)) = (l.snap.providers.iter().find(|p| p.id == id), statuses.iter().find(|s| s.info.id == id)) else {
                let known: Vec<_> = statuses.iter().map(|s| s.info.id).collect();
                return fail(EXIT_UNKNOWN_PROVIDER, "unknown_provider", format!("unknown provider. Known providers: {}", known.join(", ")), a.json);
            };
            let advice = advice(p, st);
            if a.json {
                let data = json!({
                    "provider": p,
                    "looked_in": st.path,
                    "found_locally": a.live.then_some(st.found),
                    "detection_source": if a.live { "live_collection" } else { "not_checked_cache_only" },
                    "enabled": st.enabled,
                    "contacts": st.info.endpoint,
                    "endpoint_documented": st.info.documented,
                    "key_env": st.info.key_env,
                    "advice": advice,
                });
                return (code, print(envelope("diagnose", &l, data)));
            }
            let mut out = format!("{} ({}): {}\n", p.name, p.id, status_word(p));
            if !st.path.is_empty() {
                out.push_str(&format!(
                    "  looked in: {} ({})\n",
                    st.path,
                    if !a.live {
                        "not checked; cached diagnosis"
                    } else if st.found {
                        "found"
                    } else {
                        "not found"
                    }
                ));
            }
            out.push_str(&format!("  contacts:  {}\n", st.info.endpoint.unwrap_or("nothing (local files only)")));
            for m in &p.metrics {
                out.push_str(&format!("  metric:    {} [{:?}, {:?}, {:?}]\n", describe(m, now), m.category, m.source, m.freshness));
            }
            if let Some(e) = &p.error {
                out.push_str(&format!("  error:     {:?}: {}\n", e.kind, e.message));
            }
            out.push_str(&format!("  advice:    {advice}\n"));
            (code, out)
        }
    }
}

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let (code, out) = run(&argv, &data_dir(), time::now());
    if code == 0 || code == EXIT_STALE || out.starts_with('{') || out.starts_with("quotis-cli -") {
        print!("{out}");
    } else {
        eprint!("{out}");
    }
    std::process::exit(code);
}
