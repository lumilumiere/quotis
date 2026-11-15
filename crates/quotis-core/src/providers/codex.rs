use super::{Info, Provider};
use crate::fsx::{jsonl_files, read_small, read_tail};
use crate::model::{clean, window_label, Collected, Metric, ProviderError, Source};
use crate::net::{bad_response, get, not_signed_in};
use crate::settings::{env_dir, home};
use crate::time::parse_ts;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

pub struct Codex;

static INFO: Info = Info {
    id: "codex",
    name: "Codex",
    vendor: "OpenAI",
    color: "#10a37f",
    hint: "Sign in to the Codex CLI with ChatGPT. Shows the limits Codex's /status reports; local session logs update it instantly.",
    endpoint: Some("chatgpt.com"),
    documented: false,
    key_env: None,
    default_enabled: true,
};

/// How many of the newest session files to look through for a rate-limit line.
const RECENT_FILES: usize = 20;
/// Rate-limit events are appended continuously, so the end of a file is enough.
const TAIL_BYTES: u64 = 256 << 10;

fn sessions(dir: &Path) -> PathBuf {
    dir.join("sessions")
}

/// Builds the two limit metrics. The window length comes from the data when present;
/// when it is absent the metric stays unlabelled rather than guessing "5h" or "weekly".
fn limits(windows: [(&str, Option<&Value>); 2], secs_of: impl Fn(&Value) -> Option<u64>, reset_key: &str, source: Source, observed: i64) -> Option<Collected> {
    let metrics: Vec<Metric> = windows
        .into_iter()
        .filter_map(|(id, w)| {
            let w = w?;
            let used = w.get("used_percent")?.as_f64()?;
            let secs = secs_of(w);
            let label = secs.map_or_else(|| id.to_string(), window_label);
            Some(Metric::quota_percent(id, &label, used, w.get(reset_key).and_then(Value::as_i64), source, observed).window(secs))
        })
        .collect();
    (!metrics.is_empty()).then(|| Collected { metrics, ..Default::default() })
}

pub fn parse_usage(v: &Value, now: i64) -> Option<Collected> {
    let rl = v.get("rate_limit")?;
    let secs = |w: &Value| w.get("limit_window_seconds").and_then(Value::as_u64);
    let mut c = limits([("primary", rl.get("primary_window")), ("secondary", rl.get("secondary_window"))], secs, "reset_at", Source::ProviderApi, now)?;
    c.account = v.get("plan_type").and_then(Value::as_str).map(clean).filter(|s| !s.is_empty());
    Some(c)
}

/// Finds the last rate-limit event in a session log. `fallback_ts` dates it when the line has no timestamp.
pub fn parse_log(text: &str, fallback_ts: i64) -> Option<Collected> {
    text.lines().rev().filter(|l| l.contains("\"rate_limits\"")).find_map(|line| {
        let v: Value = serde_json::from_str(line).ok()?;
        let rl = v.pointer("/payload/rate_limits")?;
        let observed = v.get("timestamp").and_then(Value::as_str).and_then(parse_ts).unwrap_or(fallback_ts);
        let secs = |w: &Value| w.get("window_minutes").and_then(Value::as_u64).map(|m| m * 60);
        limits([("primary", rl.get("primary")), ("secondary", rl.get("secondary"))], secs, "resets_at", Source::LocalLog, observed)
    })
}

impl Provider for Codex {
    fn info(&self) -> &'static Info {
        &INFO
    }

    fn default_dir(&self) -> PathBuf {
        env_dir("CODEX_HOME").unwrap_or_else(|| home().join(".codex"))
    }

    fn detect(&self, dir: &Path) -> bool {
        sessions(dir).is_dir() || dir.join("auth.json").is_file()
    }

    fn watch_dir(&self, dir: &Path) -> Option<PathBuf> {
        Some(sessions(dir))
    }

    fn collect_local(&self, dir: &Path, _now: i64) -> Option<Result<Collected, ProviderError>> {
        if !sessions(dir).is_dir() {
            return None;
        }
        // Session folders are sessions/YYYY/MM/DD, so the walk meets the newest files first.
        let mut files = jsonl_files(&sessions(dir), RECENT_FILES);
        files.sort_by_key(|(_, modified)| std::cmp::Reverse(*modified));
        let found = files.iter().find_map(|(p, modified)| {
            let mtime = modified.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
            parse_log(&read_tail(p, TAIL_BYTES).ok()?, mtime)
        });
        Some(Ok(found.unwrap_or_else(|| Collected { notes: vec!["no limit data yet".into()], ..Default::default() })))
    }

    fn collect_online(&self, dir: &Path, now: i64) -> Option<Result<Collected, ProviderError>> {
        Some((|| {
            let auth: Value = serde_json::from_str(&read_small(&dir.join("auth.json"))?).map_err(|_| not_signed_in())?;
            // An API-key login has no ChatGPT plan limits to ask about.
            let token = auth.pointer("/tokens/access_token").and_then(Value::as_str).ok_or_else(not_signed_in)?;
            let account = auth.pointer("/tokens/account_id").and_then(Value::as_str).ok_or_else(not_signed_in)?;
            let v = get(
                "https://chatgpt.com/backend-api/wham/usage",
                &[("Authorization", &format!("Bearer {token}")), ("ChatGPT-Account-Id", account), ("Accept", "application/json")],
            )?;
            parse_usage(&v, now).ok_or_else(bad_response)
        })())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOG: &str = r#"{"timestamp":"2026-09-22T09:00:00.000Z","type":"event_msg","payload":{"type":"token_count","rate_limits":{"primary":{"used_percent":1.0,"window_minutes":300,"resets_at":10},"secondary":{"used_percent":2.0,"window_minutes":10080,"resets_at":20}}}}
{"timestamp":"2026-09-22T11:30:00.000+02:00","type":"event_msg","payload":{"type":"token_count","info":null,"rate_limits":{"primary":{"used_percent":4.0,"window_minutes":300,"resets_at":1790096975},"secondary":{"used_percent":53.0,"window_minutes":10080,"resets_at":1790602389}}}}
{"type":"response_item","payload":{"type":"message","content":"a prompt mentioning \"rate_limits\" must not confuse the parser"}}
{"type":"event_msg","payload":{"type":"token_count","rate_limits":null}}
{truncated"#;

    #[test]
    fn log_takes_the_last_rate_limits_and_its_timestamp() {
        let c = parse_log(LOG, 7).unwrap();
        let (p, s) = (&c.metrics[0], &c.metrics[1]);
        assert_eq!((p.label.as_str(), p.used, p.window_seconds), ("5h", Some(4.0), Some(18_000)));
        assert_eq!((s.label.as_str(), s.resets_at), ("wk", Some(1790602389)));
        assert_eq!(p.observed_at, parse_ts("2026-09-22T09:30:00Z").unwrap());
        assert_eq!(p.source, Source::LocalLog);
        assert!(parse_log(r#"{"payload":{"rate_limits":null}}"#, 0).is_none());
        assert!(parse_log("", 0).is_none());
    }

    #[test]
    fn log_without_timestamp_uses_the_file_time() {
        let line = r#"{"payload":{"rate_limits":{"primary":{"used_percent":9.0,"resets_at":5}}}}"#;
        let c = parse_log(line, 77).unwrap();
        assert_eq!((c.metrics.len(), c.metrics[0].observed_at), (1, 77));
        // No window length in the data: no invented "5h" label.
        assert_eq!((c.metrics[0].label.as_str(), c.metrics[0].window_seconds), ("primary", None));
    }

    #[test]
    fn usage_response() {
        let v = serde_json::json!({"plan_type":"plus","rate_limit":{
            "primary_window":{"used_percent":67,"limit_window_seconds":18000,"reset_at":1790172974},
            "secondary_window":{"used_percent":94,"limit_window_seconds":604800,"reset_at":1790602390}}});
        let c = parse_usage(&v, 3).unwrap();
        assert_eq!(c.account.as_deref(), Some("plus"));
        assert_eq!((c.metrics[0].used, c.metrics[0].label.as_str()), (Some(67.0), "5h"));
        assert_eq!((c.metrics[1].resets_at, c.metrics[1].label.as_str()), (Some(1790602390), "wk"));
        let bare = serde_json::json!({"rate_limit":{"primary_window":{"used_percent":67,"reset_at":1}}});
        assert_eq!(parse_usage(&bare, 0).unwrap().metrics[0].label, "primary");
        assert!(parse_usage(&serde_json::json!({"rate_limit":null}), 0).is_none());
        assert!(parse_usage(&serde_json::json!({"rate_limit":{"primary_window":{"used_percent":"NaN"}}}), 0).is_none());
    }

    #[test]
    fn local_collection_reads_the_newest_session() {
        let dir = crate::fsx::temp_dir("codex");
        assert!(Codex.collect_local(&dir, 0).is_none());
        std::fs::create_dir_all(dir.join("sessions/2026/09/21")).unwrap();
        std::fs::create_dir_all(dir.join("sessions/2026/09/22")).unwrap();
        assert_eq!(Codex.collect_local(&dir, 0).unwrap().unwrap().notes, ["no limit data yet"]);
        std::fs::write(
            dir.join("sessions/2026/09/21/a.jsonl"),
            r#"{"timestamp":"2026-09-21T00:00:00Z","payload":{"rate_limits":{"primary":{"used_percent":1.0,"resets_at":1}}}}"#,
        )
        .unwrap();
        std::fs::write(dir.join("sessions/2026/09/22/b.jsonl"), LOG).unwrap();
        let c = Codex.collect_local(&dir, 0).unwrap().unwrap();
        assert_eq!(c.metrics[0].used, Some(4.0));
    }

    #[test]
    fn api_key_login_is_not_signed_in_for_plan_limits() {
        let dir = crate::fsx::temp_dir("codex-auth");
        std::fs::write(dir.join("auth.json"), r#"{"OPENAI_API_KEY":"sk-test"}"#).unwrap();
        let e = Codex.collect_online(&dir, 0).unwrap().unwrap_err();
        assert_eq!(e.kind, crate::model::ErrorKind::NotSignedIn);
        assert!(!format!("{e:?}").contains("sk-test"));
    }
}
