//! Contract tests for the quotis-cli binary: output shape, exit codes, shells, and the
//! guarantee that it only reads.

use quotis_core::*;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU32, Ordering};

const BIN: &str = env!("CARGO_BIN_EXE_quotis-cli");

fn temp_dir() -> PathBuf {
    static N: AtomicU32 = AtomicU32::new(0);
    let dir = std::env::temp_dir().join(format!("quotis-cli-{}-{}", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Seals the environment: every place a provider would look by default points into the
/// test folder, so no test can touch a real login or log.
fn sealed(cmd: &mut Command, dir: &Path) {
    let home = dir.join("sealed-home");
    cmd.env("QUOTIS_DATA_DIR", dir).env_remove("OPENROUTER_API_KEY").env_remove("DEEPSEEK_API_KEY");
    for var in ["HOME", "USERPROFILE", "APPDATA", "LOCALAPPDATA", "XDG_CONFIG_HOME", "CLAUDE_CONFIG_DIR", "CODEX_HOME"] {
        cmd.env(var, &home);
    }
}

fn cli(dir: &Path, args: &[&str]) -> Output {
    let mut cmd = Command::new(BIN);
    cmd.args(args);
    sealed(&mut cmd, dir);
    cmd.output().unwrap()
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn json(o: &Output) -> Value {
    serde_json::from_str(&stdout(o)).unwrap_or_else(|e| panic!("stdout is not JSON ({e}): {}", stdout(o)))
}

fn sample(generated_at: i64) -> Snapshot {
    let mut quota = Metric::quota_percent("five_hour", "5h", 38.0, Some(generated_at + 7200), Source::ProviderApi, generated_at).window(Some(18_000));
    quota.freshness = Freshness::Fresh;
    let mut expired = Metric::quota_percent("seven_day", "wk", 90.0, Some(generated_at - 10), Source::ProviderApi, generated_at - 600);
    expired.freshness = Freshness::Expired;
    let provider = |id: &str, name: &str, status, error, metrics| ProviderSnapshot {
        id: id.into(),
        name: name.into(),
        vendor: "V".into(),
        account: Some("max".into()),
        status,
        error,
        observed_at: Some(generated_at),
        refreshing: false,
        metrics,
        notes: vec![],
    };
    Snapshot {
        schema_version: SCHEMA_VERSION,
        generated_at,
        app_version: "2.0.0".into(),
        stale_after_secs: 900,
        providers: vec![
            provider("claude", "Claude Code", Status::Connected, None, vec![quota, expired]),
            provider("cursor", "Cursor", Status::Error, Some(ProviderError::new(ErrorKind::AuthExpired, "login expired, open Cursor")), vec![]),
            provider("gemini", "Gemini CLI", Status::NotDetected, None, vec![]),
        ],
    }
}

fn with_snapshot(age: i64) -> PathBuf {
    let dir = temp_dir();
    cache::write(&dir, &sample(time::now() - age)).unwrap();
    dir
}

#[test]
fn help_version_and_usage_errors() {
    let dir = temp_dir();
    let help = cli(&dir, &["--help"]);
    assert!(help.status.success() && stdout(&help).contains("EXIT CODES"));
    assert!(stdout(&cli(&dir, &["--version"])).starts_with("quotis-cli 2."));
    for bad in [
        &["frobnicate"][..],
        &["status", "--wat"],
        &["diagnose"],
        &["status", "--offline"],
        &["export", "--format", "xml"],
        &["status", "--max-age", "soon"],
        &["status", "extra"],
    ] {
        let o = cli(&dir, bad);
        assert_eq!(o.status.code(), Some(2), "{bad:?}");
        assert!(String::from_utf8_lossy(&o.stderr).contains("error:"), "{bad:?}");
    }
    assert_eq!(cli(&dir, &[]).status.code(), Some(2));
}

#[test]
fn missing_desktop_snapshot_is_exit_3_with_a_json_error() {
    let dir = temp_dir();
    let o = cli(&dir, &["status", "--json"]);
    assert_eq!(o.status.code(), Some(3));
    let v = json(&o);
    assert_eq!(v["error"]["code"], "no_snapshot");
    assert_eq!(v["schema_version"], 1);
    let text = cli(&dir, &["status"]);
    assert_eq!(text.status.code(), Some(3));
    assert!(String::from_utf8_lossy(&text.stderr).contains("--live"));
}

#[test]
fn malformed_and_incompatible_snapshots() {
    let dir = temp_dir();
    std::fs::write(cache::path(&dir), "{ this is not json").unwrap();
    let o = cli(&dir, &["snapshot"]);
    assert_eq!((o.status.code(), json(&o)["error"]["code"].as_str()), (Some(6), Some("malformed_snapshot")));
    std::fs::write(cache::path(&dir), r#"{"schema_version":2,"generated_at":1,"app_version":"3.0.0","providers":[]}"#).unwrap();
    let o = cli(&dir, &["snapshot"]);
    assert_eq!((o.status.code(), json(&o)["error"]["code"].as_str()), (Some(5), Some("incompatible_schema")));
    std::fs::write(cache::path(&dir), vec![b'['; 5 << 20]).unwrap();
    assert_eq!(cli(&dir, &["snapshot"]).status.code(), Some(6), "an oversized file is rejected, not parsed");
}

#[test]
fn snapshot_envelope_is_stable() {
    let dir = with_snapshot(5);
    let o = cli(&dir, &["snapshot"]);
    assert!(o.status.success());
    let v = json(&o);
    assert_eq!(
        (v["schema_version"].as_i64(), v["command"].as_str(), v["source"].as_str(), v["stale"].as_bool()),
        (Some(1), Some("snapshot"), Some("desktop_cache"), Some(false))
    );
    assert!(v["age_seconds"].as_i64().unwrap() < 60 && v["generated_at"].is_i64());
    let m = &v["data"]["providers"][0]["metrics"][0];
    for field in ["id", "label", "category", "unit", "used", "remaining", "total", "resets_at", "observed_at", "source", "freshness", "window_seconds"] {
        assert!(!m[field].is_null(), "metric.{field} missing: {m}");
    }
    assert_eq!((m["category"].as_str(), m["source"].as_str(), m["freshness"].as_str()), (Some("provider_quota"), Some("provider_api"), Some("fresh")));
    assert_eq!(v["data"]["providers"][0]["metrics"][1]["freshness"], "expired");
    assert_eq!(v["data"]["providers"][1]["error"]["kind"], "auth_expired");
}

#[test]
fn stale_snapshot_still_prints_but_exits_4() {
    let dir = with_snapshot(1000);
    let o = cli(&dir, &["status", "--json"]);
    assert_eq!(o.status.code(), Some(4));
    let v = json(&o);
    assert_eq!(v["stale"], true);
    assert!(v["age_seconds"].as_i64().unwrap() >= 1000);
    assert!(cli(&dir, &["status", "--json", "--max-age", "5000"]).status.success());
    let text = stdout(&cli(&dir, &["status"]));
    assert!(text.contains("Warning: this snapshot is"));
    // A snapshot from the future means the clock changed: stale, not "fresh forever".
    let future = with_snapshot(-4000);
    assert_eq!(cli(&future, &["status", "--json"]).status.code(), Some(4));
}

#[test]
fn freshness_is_judged_when_read_not_when_written() {
    // Written 15 seconds ago and marked fresh, but the window reset 10 seconds ago.
    let dir = temp_dir();
    let mut snap = sample(time::now() - 15);
    snap.providers[0].metrics[0].resets_at = Some(time::now() - 10);
    assert_eq!(snap.providers[0].metrics[0].freshness, Freshness::Fresh);
    cache::write(&dir, &snap).unwrap();
    let o = cli(&dir, &["snapshot"]);
    assert!(o.status.success(), "the snapshot itself is recent");
    assert_eq!(json(&o)["data"]["providers"][0]["metrics"][0]["freshness"], "expired");
    let text = stdout(&cli(&dir, &["status"]));
    assert!(text.contains("window reset since this was read") && !text.contains("5h 62% left (resets"), "{text}");

    // An old snapshot: its numbers are stale as well as the file.
    let old = with_snapshot(2000);
    assert_eq!(json(&cli(&old, &["snapshot"]))["data"]["providers"][0]["metrics"][0]["freshness"], "stale");

    // Options that a command would ignore are refused rather than silently accepted.
    assert_eq!(cli(&dir, &["status", "--provider", "claude"]).status.code(), Some(2));
    assert_eq!(cli(&dir, &["snapshot", "--format", "json"]).status.code(), Some(2));
}

#[test]
fn status_text_qualifies_expired_and_hides_undetected() {
    let dir = with_snapshot(5);
    let text = stdout(&cli(&dir, &["status"]));
    assert!(text.contains("Claude Code") && text.contains("5h 62% left (resets in 1h 59m)"), "{text}");
    assert!(text.contains("window reset since this was read"), "{text}");
    assert!(text.contains("Cursor") && text.contains("login expired"), "{text}");
    assert!(!text.contains("Gemini"), "{text}");
}

#[test]
fn diagnose_explains_and_rejects_unknown_ids() {
    let dir = with_snapshot(5);
    let o = cli(&dir, &["diagnose", "--provider", "cursor", "--json"]);
    assert!(o.status.success());
    let v = json(&o);
    assert_eq!(v["data"]["contacts"], "cursor.com");
    assert_eq!(v["data"]["endpoint_documented"], false);
    assert!(v["data"]["advice"].as_str().unwrap().contains("never refreshes logins"));
    let text = stdout(&cli(&dir, &["diagnose", "--provider", "claude"]));
    assert!(text.contains("advice:") && text.contains("stale or their window has reset"), "{text}");
    for bad in ["nope", "../../etc/passwd", "claude; rm -rf /", ""] {
        let o = cli(&dir, &["diagnose", "--provider", bad, "--json"]);
        assert_eq!(o.status.code(), Some(7), "{bad:?}");
        let msg = json(&o)["error"]["message"].as_str().unwrap().to_string();
        assert!(!msg.contains(bad) || bad.is_empty(), "the bad id is not echoed back: {msg}");
    }
}

#[test]
fn export_is_sanitized() {
    let dir = with_snapshot(5);
    let v = json(&cli(&dir, &["export", "--format", "json"]));
    assert_eq!(v["quotis_export"], 1);
    assert!(v["snapshot"]["providers"].as_array().unwrap().iter().all(|p| p.get("account").is_none()));
    assert_eq!(v["snapshot"]["schema_version"], 1);
}

#[test]
fn providers_lists_the_registry_from_a_sealed_environment() {
    let dir = temp_dir();
    // A fake Copilot login where the sealed environment would look for one. It must be
    // found (so we know the sandbox is what was read) and must never be printed.
    let copilot = if cfg!(windows) { dir.join("sealed-home/github-copilot") } else { dir.join("sealed-home/.config/github-copilot") };
    std::fs::create_dir_all(&copilot).unwrap();
    std::fs::write(copilot.join("apps.json"), r#"{"github.com:Iv1":{"oauth_token":"gho_FAKE_SANDBOX_TOKEN"}}"#).unwrap();
    let out = stdout(&cli(&dir, &["providers", "--json"]));
    assert!(!out.contains("gho_FAKE_SANDBOX_TOKEN"));
    let found: Value = serde_json::from_str(&out).unwrap();
    let row = |id: &str| found["data"].as_array().unwrap().iter().find(|r| r["id"] == id).unwrap().clone();
    assert_eq!(row("copilot")["found"], true, "the sandboxed login is the one that was seen");
    assert_eq!(row("claude")["found"], false, "nothing outside the sandbox was seen");
    assert!(row("copilot")["path"].as_str().unwrap().contains("sealed-home"));
    let v = json(&cli(&dir, &["providers", "--json"]));
    let rows = v["data"].as_array().unwrap();
    let ids: Vec<_> = rows.iter().map(|r| r["id"].as_str().unwrap()).collect();
    assert_eq!(ids, ["claude", "codex", "copilot", "cursor", "gemini", "qwen", "opencode", "openrouter", "deepseek"]);
    let openrouter = rows.iter().find(|r| r["id"] == "openrouter").unwrap();
    assert_eq!(
        (openrouter["enabled"].as_bool(), openrouter["documented"].as_bool(), openrouter["key_env"].as_str()),
        (Some(false), Some(true), Some("OPENROUTER_API_KEY"))
    );
    assert!(stdout(&cli(&dir, &["providers"])).contains("undocumented endpoint"));
}

/// Settings that point every file-based provider at an empty folder, except the ones given.
fn isolated_settings(dir: &Path, real: &[(&str, &Path)]) {
    let mut s = Settings::default();
    for id in ["claude", "codex", "copilot", "cursor", "gemini", "qwen", "opencode"] {
        let folder = real.iter().find(|(r, _)| *r == id).map_or(dir.join("empty"), |(_, p)| p.to_path_buf());
        s.providers.insert(id.into(), ProviderCfg { enabled: true, dir: folder.display().to_string() });
    }
    s.save(dir).unwrap();
}

#[test]
fn live_local_only_reads_fixtures_and_writes_nothing() {
    let dir = temp_dir();
    let codex = dir.join("codex-home");
    std::fs::create_dir_all(codex.join("sessions/2026/11/15")).unwrap();
    // A login file with a fake token: it must never appear in any output.
    std::fs::write(codex.join("auth.json"), r#"{"tokens":{"access_token":"sk-FAKE-TOKEN-123","account_id":"acct-FAKE"}}"#).unwrap();
    let stamp = "2026-11-15T10:00:00Z";
    std::fs::write(
        codex.join("sessions/2026/11/15/rollout.jsonl"),
        format!(r#"{{"timestamp":"{stamp}","payload":{{"prompt":"SECRET PROMPT TEXT","rate_limits":{{"primary":{{"used_percent":25.0,"window_minutes":300,"resets_at":4102444800}}}}}}}}"#),
    )
    .unwrap();
    isolated_settings(&dir, &[("codex", &codex)]);
    let before = walk(&dir);

    let o = cli(&dir, &["snapshot", "--live", "--offline"]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let out = stdout(&o);
    let v = json(&o);
    assert_eq!(v["source"], "live_local_only");
    let codex_row = v["data"]["providers"].as_array().unwrap().iter().find(|p| p["id"] == "codex").unwrap();
    assert_eq!(codex_row["status"], "connected");
    assert_eq!(codex_row["metrics"][0]["remaining"], 75.0);
    assert_eq!(codex_row["metrics"][0]["source"], "local_log");
    assert_eq!(codex_row["metrics"][0]["observed_at"], time::parse_ts(stamp).unwrap());
    for secret in ["sk-FAKE-TOKEN-123", "acct-FAKE", "SECRET PROMPT TEXT"] {
        assert!(!out.contains(secret), "output leaked {secret}");
    }
    assert_eq!(walk(&dir), before, "a read-only CLI leaves the data folder untouched");
    assert!(!cache::path(&dir).exists(), "--live does not write the desktop's snapshot");
}

/// Every file under `dir` with its exact bytes and modification time.
fn walk(dir: &Path) -> Vec<(PathBuf, Vec<u8>, std::time::SystemTime)> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        if e.path().is_dir() {
            out.extend(walk(&e.path()));
        } else {
            out.push((e.path(), std::fs::read(e.path()).unwrap(), e.metadata().unwrap().modified().unwrap()));
        }
    }
    out.sort();
    out
}

#[test]
fn works_from_the_platform_shells() {
    let dir = with_snapshot(5);
    let shells: &[(&str, &[&str])] = if cfg!(windows) {
        &[("cmd", &["/C"]), ("powershell", &["-NoProfile", "-Command"]), ("pwsh", &["-NoProfile", "-Command"])]
    } else {
        &[("sh", &["-c"]), ("bash", &["-c"]), ("zsh", &["-c"])]
    };
    let mut ran = 0;
    for (shell, flags) in shells {
        let line = if *shell == "cmd" {
            format!("\"{BIN}\" status --json")
        } else if cfg!(windows) {
            format!("& '{BIN}' status --json")
        } else {
            format!("'{BIN}' status --json")
        };
        let mut cmd = Command::new(shell);
        cmd.args(*flags).env("QUOTIS_DATA_DIR", &dir);
        // cmd.exe has its own quoting rules, so its command line is passed through untouched.
        #[cfg(windows)]
        if *shell == "cmd" {
            std::os::windows::process::CommandExt::raw_arg(&mut cmd, &line);
        } else {
            cmd.arg(&line);
        }
        #[cfg(not(windows))]
        cmd.arg(&line);
        let Ok(o) = cmd.output() else { continue };
        ran += 1;
        assert!(o.status.success(), "{shell}: {}", String::from_utf8_lossy(&o.stderr));
        assert_eq!(json(&o)["command"], "status", "{shell}");
    }
    assert!(ran >= 1, "no shell available to test");
}

#[test]
fn concurrent_readers_during_rewrites_never_see_a_torn_file() {
    let dir = with_snapshot(5);
    let writer_dir = dir.clone();
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let stop_w = stop.clone();
    let writer = std::thread::spawn(move || {
        while !stop_w.load(Ordering::Relaxed) {
            cache::write(&writer_dir, &sample(time::now())).unwrap();
        }
    });
    let readers: Vec<_> = (0..4)
        .map(|_| {
            let dir = dir.clone();
            std::thread::spawn(move || (0..15).map(|_| cli(&dir, &["snapshot"]).status.code()).collect::<Vec<_>>())
        })
        .collect();
    for r in readers {
        for code in r.join().unwrap() {
            assert_eq!(code, Some(0), "a reader saw a missing or half-written snapshot");
        }
    }
    stop.store(true, Ordering::Relaxed);
    writer.join().unwrap();
}
