use super::{Info, Provider};
use crate::fsx::read_small;
use crate::model::{clean, window_label, Collected, ErrorKind, Metric, ProviderError, Source};
use crate::net::{bad_response, get, not_signed_in};
use crate::settings::{env_dir, home};
use crate::time::parse_ts;
use serde_json::Value;
use std::path::{Path, PathBuf};

pub struct Claude;

static INFO: Info = Info {
    id: "claude",
    name: "Claude Code",
    vendor: "Anthropic",
    color: "#d97757",
    hint: "Sign in to Claude Code with a Pro or Max plan. Shows the limits Claude Code's /usage reports.",
    endpoint: Some("api.anthropic.com"),
    documented: false,
    key_env: None,
    default_enabled: true,
};

/// Windows the usage endpoint is known to report, with the duration its name states.
const WINDOWS: [(&str, u64, &str); 4] =
    [("five_hour", 18_000, ""), ("seven_day", 604_800, ""), ("seven_day_opus", 604_800, "opus "), ("seven_day_sonnet", 604_800, "sonnet ")];

fn credentials_file(root: &Path) -> PathBuf {
    root.join(".credentials.json")
}

fn credentials(root: &Path) -> Result<Value, ProviderError> {
    let text = match read_small(&credentials_file(root)) {
        Ok(t) => t,
        Err(e) if e.kind == ErrorKind::NotSignedIn => keychain()?,
        Err(e) => return Err(e),
    };
    let v: Value = serde_json::from_str(&text).map_err(|_| not_signed_in())?;
    v.get("claudeAiOauth").is_some().then_some(v).ok_or_else(not_signed_in)
}

/// macOS keeps the Claude Code login in the Keychain; the OS asks the user before releasing it.
#[cfg(target_os = "macos")]
fn keychain() -> Result<String, ProviderError> {
    let out = std::process::Command::new("/usr/bin/security")
        .args(["find-generic-password", "-s", "Claude Code-credentials", "-w"])
        .output()
        .map_err(|_| not_signed_in())?;
    if out.status.success() {
        return Ok(String::from_utf8_lossy(&out.stdout).trim().to_string());
    }
    // 44 = item not found; anything else means the user or the OS refused access.
    match out.status.code() {
        Some(44) => Err(not_signed_in()),
        _ => Err(ProviderError::new(ErrorKind::PermissionDenied, "Keychain access to the Claude Code login was denied")),
    }
}

#[cfg(not(target_os = "macos"))]
fn keychain() -> Result<String, ProviderError> {
    Err(not_signed_in())
}

pub fn parse_usage(v: &Value, now: i64) -> Option<Collected> {
    let metrics: Vec<Metric> = WINDOWS
        .iter()
        .filter_map(|&(key, secs, prefix)| {
            let w = v.get(key)?;
            let used = w.get("utilization")?.as_f64()?;
            let resets = w.get("resets_at").and_then(Value::as_str).and_then(parse_ts);
            let label = format!("{prefix}{}", window_label(secs));
            Some(Metric::quota_percent(key, &label, used, resets, Source::ProviderApi, now).window(Some(secs)))
        })
        .collect();
    (!metrics.is_empty()).then(|| Collected { metrics, ..Default::default() })
}

impl Provider for Claude {
    fn info(&self) -> &'static Info {
        &INFO
    }

    fn default_dir(&self) -> PathBuf {
        env_dir("CLAUDE_CONFIG_DIR").unwrap_or_else(|| home().join(".claude"))
    }

    fn detect(&self, dir: &Path) -> bool {
        // On macOS the login lives in the Keychain; asking for it here would prompt the user
        // on every Settings open, so an existing Claude folder counts as detected.
        credentials_file(dir).is_file() || (cfg!(target_os = "macos") && dir.join("projects").is_dir())
    }

    fn watch_dir(&self, dir: &Path) -> Option<PathBuf> {
        Some(dir.join("projects"))
    }

    fn collect_online(&self, dir: &Path, now: i64) -> Option<Result<Collected, ProviderError>> {
        Some((|| {
            let creds = credentials(dir)?;
            let oauth = &creds["claudeAiOauth"];
            let token = oauth.get("accessToken").and_then(Value::as_str).ok_or_else(not_signed_in)?;
            if oauth.get("expiresAt").and_then(Value::as_i64).is_some_and(|ms| ms / 1000 < now) {
                return Err(ProviderError::new(ErrorKind::AuthExpired, "login expired, open Claude Code"));
            }
            let v = get("https://api.anthropic.com/api/oauth/usage", &[("Authorization", &format!("Bearer {token}")), ("anthropic-beta", "oauth-2025-04-20")])
                .map_err(|e| if e.kind == ErrorKind::AuthExpired { ProviderError::new(e.kind, "login expired, open Claude Code") } else { e })?;
            let mut c = parse_usage(&v, now).ok_or_else(bad_response)?;
            c.account = oauth.get("subscriptionType").and_then(Value::as_str).map(clean).filter(|s| !s.is_empty());
            Ok(c)
        })())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Category;

    #[test]
    fn usage_response() {
        let v = serde_json::json!({"five_hour":{"utilization":12.0,"resets_at":"2026-09-22T14:00:00.5+00:00"},
            "seven_day":{"utilization":35.5,"resets_at":null},"seven_day_opus":null,"extra":{"utilization":"x"}});
        let c = parse_usage(&v, 100).unwrap();
        assert_eq!(c.metrics.len(), 2);
        let (five, week) = (&c.metrics[0], &c.metrics[1]);
        assert_eq!((five.id.as_str(), five.label.as_str(), five.used, five.remaining), ("five_hour", "5h", Some(12.0), Some(88.0)));
        assert_eq!(five.resets_at, parse_ts("2026-09-22T14:00:00Z"));
        assert_eq!((five.category, five.window_seconds, five.observed_at), (Category::ProviderQuota, Some(18_000), 100));
        assert_eq!((week.label.as_str(), week.resets_at), ("wk", None));
    }

    #[test]
    fn reset_time_honours_its_offset() {
        let v = serde_json::json!({"five_hour":{"utilization":1.0,"resets_at":"2026-09-22T19:30:00+05:30"}});
        assert_eq!(parse_usage(&v, 0).unwrap().metrics[0].resets_at, parse_ts("2026-09-22T14:00:00Z"));
    }

    #[test]
    fn model_specific_windows_and_garbage() {
        let v = serde_json::json!({"seven_day_opus":{"utilization":150.0,"resets_at":"nope"}});
        let m = &parse_usage(&v, 0).unwrap().metrics[0];
        assert_eq!((m.label.as_str(), m.used, m.resets_at), ("opus wk", Some(100.0), None));
        assert!(parse_usage(&serde_json::json!({"error":"x"}), 0).is_none());
        assert!(parse_usage(&serde_json::json!([1, 2]), 0).is_none());
    }

    #[test]
    fn expired_login_never_reaches_the_network() {
        let dir = crate::fsx::temp_dir("claude");
        std::fs::write(credentials_file(&dir), r#"{"claudeAiOauth":{"accessToken":"sk-test","expiresAt":1000}}"#).unwrap();
        let e = Claude.collect_online(&dir, 2_000_000).unwrap().unwrap_err();
        assert_eq!(e.kind, ErrorKind::AuthExpired);
        assert!(!e.message.contains("sk-test"));
        std::fs::write(credentials_file(&dir), "{not json").unwrap();
        assert_eq!(Claude.collect_online(&dir, 0).unwrap().unwrap_err().kind, ErrorKind::NotSignedIn);
    }
}
