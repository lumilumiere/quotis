use super::{Info, Provider};
use crate::fsx::read_small;
use crate::model::{clean, Collected, ErrorKind, Metric, ProviderError, Source};
use crate::net::{bad_response, get, not_signed_in};
use crate::settings::{env_dir, home};
use crate::time::parse_ts;
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub struct Copilot;

static INFO: Info = Info {
    id: "copilot",
    name: "GitHub Copilot",
    vendor: "GitHub",
    color: "#8957e5",
    hint: "Sign in to Copilot in VS Code or a JetBrains IDE. Shows premium requests left and when they reset.",
    endpoint: Some("api.github.com"),
    documented: false,
    key_env: None,
    default_enabled: true,
};

fn tokens(dir: &Path) -> Vec<String> {
    ["apps.json", "hosts.json"]
        .iter()
        .filter_map(|f| read_small(&dir.join(f)).ok())
        .filter_map(|t| serde_json::from_str::<HashMap<String, Value>>(&t).ok())
        .flat_map(|m| m.into_iter())
        // Only github.com logins: a GitHub Enterprise token must never be sent to api.github.com.
        .filter(|(host, _)| host == "github.com" || host.starts_with("github.com:"))
        .map(|(_, v)| v)
        .filter_map(|v| v.get("oauth_token")?.as_str().map(String::from))
        .collect()
}

pub fn parse(v: &Value, now: i64) -> Option<Collected> {
    let p = v.pointer("/quota_snapshots/premium_interactions")?;
    let account = v.get("copilot_plan").and_then(Value::as_str).map(clean).filter(|s| !s.is_empty());
    if p.get("unlimited").and_then(Value::as_bool) == Some(true) {
        return Some(Collected { notes: vec!["unlimited premium requests".into()], account, ..Default::default() });
    }
    let left = p.get("percent_remaining")?.as_f64()?;
    let resets = v
        .get("quota_reset_date_utc")
        .or_else(|| v.get("quota_reset_date"))
        .and_then(Value::as_str)
        .and_then(|s| parse_ts(s).or_else(|| parse_ts(&format!("{s}T00:00:00Z"))));
    let metric = Metric::quota_percent("premium_requests", "premium", 100.0 - left, resets, Source::ProviderApi, now);
    Some(Collected { metrics: vec![metric], account, ..Default::default() })
}

impl Provider for Copilot {
    fn info(&self) -> &'static Info {
        &INFO
    }

    fn default_dir(&self) -> PathBuf {
        if cfg!(windows) {
            env_dir("LOCALAPPDATA").unwrap_or_else(|| home().join("AppData/Local")).join("github-copilot")
        } else {
            home().join(".config/github-copilot")
        }
    }

    fn detect(&self, dir: &Path) -> bool {
        !tokens(dir).is_empty()
    }

    fn collect_online(&self, dir: &Path, now: i64) -> Option<Result<Collected, ProviderError>> {
        let tokens = tokens(dir);
        if tokens.is_empty() {
            return Some(Err(not_signed_in()));
        }
        let mut err = ProviderError::new(ErrorKind::AuthExpired, "login expired, sign in to Copilot again");
        for t in tokens {
            match get("https://api.github.com/copilot_internal/user", &[("Authorization", &format!("token {t}")), ("Accept", "application/json")]) {
                Ok(v) => return Some(parse(&v, now).ok_or_else(bad_response)),
                Err(e) if e.kind == ErrorKind::AuthExpired => continue,
                Err(e) => err = e,
            }
        }
        Some(Err(err))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn premium_requests() {
        let v = serde_json::json!({"copilot_plan":"individual","quota_reset_date":"2026-10-01","quota_reset_date_utc":"2026-10-01T00:00:00.000Z",
            "quota_snapshots":{"premium_interactions":{"percent_remaining":37.5,"unlimited":false,"entitlement":300}}});
        let c = parse(&v, 9).unwrap();
        let m = &c.metrics[0];
        assert_eq!((m.used, m.remaining, m.resets_at), (Some(62.5), Some(37.5), parse_ts("2026-10-01T00:00:00Z")));
        assert_eq!((m.window_seconds, c.account.as_deref()), (None, Some("individual")));
    }

    #[test]
    fn overage_date_only_reset_and_unlimited() {
        let over = serde_json::json!({"quota_reset_date":"2026-10-01","quota_snapshots":{"premium_interactions":{"percent_remaining":-1.0,"unlimited":false}}});
        let m = &parse(&over, 0).unwrap().metrics[0];
        assert_eq!((m.used, m.resets_at), (Some(100.0), parse_ts("2026-10-01T00:00:00Z")));
        let unl = serde_json::json!({"quota_snapshots":{"premium_interactions":{"unlimited":true}}});
        let c = parse(&unl, 0).unwrap();
        assert!(c.metrics.is_empty() && c.notes == ["unlimited premium requests"]);
        assert!(parse(&serde_json::json!({"quota_snapshots":{}}), 0).is_none());
        let bad_date = serde_json::json!({"quota_reset_date":"soon","quota_snapshots":{"premium_interactions":{"percent_remaining":10.0}}});
        assert_eq!(parse(&bad_date, 0).unwrap().metrics[0].resets_at, None);
    }

    #[test]
    fn tokens_come_from_both_files_and_survive_garbage() {
        let dir = crate::fsx::temp_dir("copilot");
        std::fs::write(
            dir.join("apps.json"),
            r#"{"github.com:Iv1":{"oauth_token":"gho_a"},"ghe.corp.example:Iv1":{"oauth_token":"gho_enterprise"},"x":{"user":"u"}}"#,
        )
        .unwrap();
        std::fs::write(dir.join("hosts.json"), "[]").unwrap();
        assert_eq!(tokens(&dir), ["gho_a"]);
        assert!(Copilot.detect(&dir));
    }
}
