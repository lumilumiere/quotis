use super::{Info, Provider};
use crate::model::{clean, Collected, Metric, ProviderError, Source};
use crate::net::{bad_response, get, not_signed_in};
use crate::settings::config_base;
use crate::time::parse_ts;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub struct Cursor;

static INFO: Info = Info {
    id: "cursor",
    name: "Cursor",
    vendor: "Anysphere",
    color: "#c9ccd1",
    hint: "Sign in to the Cursor app. Shows how much of your plan's included usage is left this billing cycle.",
    endpoint: Some("cursor.com"),
    documented: false,
    key_env: None,
    default_enabled: true,
};

pub(crate) fn open_ro(path: &Path) -> Option<rusqlite::Connection> {
    use rusqlite::OpenFlags;
    if !path.is_file() {
        return None;
    }
    let c = rusqlite::Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX).ok()?;
    let _ = c.busy_timeout(Duration::from_millis(500));
    Some(c)
}

fn db(dir: &Path) -> PathBuf {
    dir.join("state.vscdb")
}

fn token(db: &Path) -> Option<String> {
    open_ro(db)?.query_row("SELECT value FROM ItemTable WHERE key = 'cursorAuth/accessToken'", [], |r| r.get::<_, String>(0)).ok().filter(|t| !t.is_empty())
}

/// The user id inside the login token's `sub` claim (`provider|user_id`).
fn user_id(token: &str) -> Option<String> {
    let payload = token.split('.').nth(1)?;
    let claims: Value = serde_json::from_slice(&base64url(payload)?).ok()?;
    let user = claims.get("sub")?.as_str()?.rsplit('|').next()?;
    // The id goes into a cookie header, so it must be a plain token.
    (!user.is_empty() && user.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')).then(|| user.to_string())
}

pub fn parse(v: &Value, now: i64) -> Option<Collected> {
    let account = v.get("membershipType").and_then(Value::as_str).map(clean).filter(|s| !s.is_empty());
    if v.get("isUnlimited").and_then(Value::as_bool) == Some(true) {
        return Some(Collected { notes: vec!["unlimited plan".into()], account, ..Default::default() });
    }
    let plan = v.pointer("/individualUsage/plan")?;
    if plan.get("limit").and_then(Value::as_f64) == Some(0.0) {
        return Some(Collected { notes: vec!["plan has no included usage".into()], account, ..Default::default() });
    }
    let used = plan.get("totalPercentUsed")?.as_f64()?;
    let resets = v.get("billingCycleEnd").and_then(Value::as_str).and_then(parse_ts);
    let metric = Metric::quota_percent("plan", "plan", used, resets, Source::ProviderApi, now);
    Some(Collected { metrics: vec![metric], account, ..Default::default() })
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
        acc = ((acc << 6) | val(c)? as u32) & 0xFFFF;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Some(out)
}

impl Provider for Cursor {
    fn info(&self) -> &'static Info {
        &INFO
    }

    fn default_dir(&self) -> PathBuf {
        config_base().join("Cursor/User/globalStorage")
    }

    fn detect(&self, dir: &Path) -> bool {
        token(&db(dir)).is_some()
    }

    fn collect_online(&self, dir: &Path, now: i64) -> Option<Result<Collected, ProviderError>> {
        Some((|| {
            let token = token(&db(dir)).ok_or_else(not_signed_in)?;
            let user = user_id(&token).ok_or_else(not_signed_in)?;
            if !token.bytes().all(|b| b.is_ascii_graphic() && b != b';' && b != b',') {
                return Err(not_signed_in());
            }
            let v = get("https://cursor.com/api/usage-summary", &[("Cookie", &format!("WorkosCursorSessionToken={user}%3A%3A{token}"))]).map_err(|e| {
                if e.kind == crate::model::ErrorKind::AuthExpired {
                    ProviderError::new(e.kind, "login expired, open Cursor")
                } else {
                    e
                }
            })?;
            parse(&v, now).ok_or_else(bad_response)
        })())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usage_summary() {
        let v = serde_json::json!({"billingCycleEnd":"2026-10-01T09:23:29.338Z","membershipType":"pro","isUnlimited":false,
            "individualUsage":{"plan":{"used":1200,"limit":2000,"totalPercentUsed":60.0}}});
        let c = parse(&v, 4).unwrap();
        assert_eq!((c.metrics[0].used, c.metrics[0].resets_at), (Some(60.0), parse_ts("2026-10-01T09:23:29Z")));
        assert_eq!(c.account.as_deref(), Some("pro"));
    }

    #[test]
    fn plans_without_a_bar_and_hostile_plan_names() {
        let free = serde_json::json!({"membershipType":"free","isUnlimited":false,"individualUsage":{"plan":{"limit":0,"totalPercentUsed":0}}});
        let c = parse(&free, 0).unwrap();
        assert!(c.metrics.is_empty() && c.notes == ["plan has no included usage"]);
        let unl = serde_json::json!({"isUnlimited":true});
        assert_eq!(parse(&unl, 0).unwrap().notes, ["unlimited plan"]);
        // A provider-controlled string is never trusted: control characters go, length is capped.
        let evil = serde_json::json!({"membershipType":"<img src=x onerror=alert(1)>\u{0007}\n".repeat(20),"isUnlimited":true});
        let account = parse(&evil, 0).unwrap().account.unwrap();
        assert!(account.chars().count() <= 120 && !account.chars().any(char::is_control));
        assert!(parse(&serde_json::json!({}), 0).is_none());
    }

    #[test]
    fn jwt_user_id() {
        assert_eq!(user_id("h.eyJzdWIiOiJhdXRoMHx1c2VyXzEyMyJ9.s").as_deref(), Some("user_123"));
        // {"sub":"auth0|a;b=c"} must not be allowed into a cookie header.
        assert_eq!(user_id("h.eyJzdWIiOiJhdXRoMHxhO2I9YyJ9.s"), None);
        assert_eq!(user_id("not-a-jwt"), None);
        assert_eq!(user_id("h.@@@.s"), None);
    }

    #[test]
    fn token_is_read_from_a_read_only_database() {
        let dir = crate::fsx::temp_dir("cursor");
        assert!(!Cursor.detect(&dir));
        let c = rusqlite::Connection::open(db(&dir)).unwrap();
        c.execute_batch(
            "CREATE TABLE ItemTable (key TEXT, value TEXT); INSERT INTO ItemTable VALUES ('cursorAuth/accessToken', 'h.eyJzdWIiOiJhdXRoMHx1c2VyXzEyMyJ9.s')",
        )
        .unwrap();
        drop(c);
        assert!(Cursor.detect(&dir));
        assert!(open_ro(&db(&dir)).unwrap().execute("DELETE FROM ItemTable", []).is_err());
    }
}
