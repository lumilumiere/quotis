//! Providers with a documented account API and no local login: the user supplies an API key
//! through an environment variable. Quotis reads the key at request time and never stores it.

use super::{Info, Provider};
use crate::model::{clean, Category, Collected, Metric, ProviderError, Source, Unit};
use crate::net::{bad_response, get, not_signed_in};
use crate::time::parse_ts;
use serde_json::Value;
use std::path::Path;

fn key(info: &Info) -> Option<String> {
    let k = std::env::var(info.key_env?).ok()?;
    let k = k.trim();
    (!k.is_empty() && k.bytes().all(|b| b.is_ascii_graphic())).then(|| k.to_string())
}

fn fetch(info: &'static Info, url: &str) -> Result<Value, ProviderError> {
    let key = key(info).ok_or_else(not_signed_in)?;
    get(url, &[("Authorization", &format!("Bearer {key}")), ("Accept", "application/json")])
}

pub struct OpenRouter;

static OPENROUTER: Info = Info {
    id: "openrouter",
    name: "OpenRouter",
    vendor: "OpenRouter",
    color: "#6467f2",
    hint: "Set OPENROUTER_API_KEY in your environment, then turn this on. Shows credits used and any limit set on the key.",
    endpoint: Some("openrouter.ai"),
    documented: true,
    key_env: Some("OPENROUTER_API_KEY"),
    default_enabled: false,
};

/// `GET /api/v1/key`. Credits are US dollars.
pub fn parse_openrouter(v: &Value, now: i64) -> Option<Collected> {
    let d = v.get("data")?;
    let api = |id: &str, label: &str, cat| Metric::new(id, label, cat, Unit::Usd, Source::ProviderApi, now);
    let mut metrics = Vec::new();
    for (field, id, label, secs) in [("usage_daily", "credits_day", "day", 86_400), ("usage_monthly", "credits_month", "mo", 0)] {
        if let Some(used) = d.get(field).and_then(Value::as_f64) {
            metrics.push(
                Metric { used: Some(used), qualification: Some("current UTC period".into()), ..api(id, label, Category::ApiConsumption) }.window(Some(secs)),
            );
        }
    }
    // A key limit is the only allowance OpenRouter reports; null means the key is uncapped.
    if let (Some(limit), Some(left)) = (d.get("limit").and_then(Value::as_f64), d.get("limit_remaining").and_then(Value::as_f64)) {
        metrics.push(Metric { total: Some(limit), remaining: Some(left), used: Some(limit - left), ..api("key_limit", "limit", Category::ProviderQuota) });
    }
    // The budget that blocks requests first carries its own reset time.
    if let Some(b) = d.get("effective_budget").filter(|b| b.is_object()) {
        if let (Some(limit), Some(left)) = (b.get("limit_usd").and_then(Value::as_f64), b.get("remaining_usd").and_then(Value::as_f64)) {
            let resets_at = b.get("resets_at").and_then(Value::as_str).and_then(parse_ts);
            metrics.push(Metric {
                total: Some(limit),
                remaining: Some(left),
                used: b.get("spend_usd").and_then(Value::as_f64),
                resets_at,
                ..api("budget", "budget", Category::ProviderQuota)
            });
        }
    }
    if let Some(f) = d.get("free_model_daily_requests") {
        if let (Some(limit), Some(left)) = (f.get("limit").and_then(Value::as_f64), f.get("remaining").and_then(Value::as_f64)) {
            let m = Metric::new("free_requests", "free/day", Category::ProviderQuota, Unit::Requests, Source::ProviderApi, now);
            metrics.push(Metric { total: Some(limit), remaining: Some(left), used: f.get("used").and_then(Value::as_f64), ..m }.window(Some(86_400)));
        }
    }
    let mut notes = Vec::new();
    if d.get("limit").is_some_and(Value::is_null) {
        notes.push("no credit limit on this key".into());
    }
    (!metrics.is_empty()).then_some(Collected { metrics, notes, account: None })
}

impl Provider for OpenRouter {
    fn info(&self) -> &'static Info {
        &OPENROUTER
    }
    fn detect(&self, _dir: &Path) -> bool {
        key(&OPENROUTER).is_some()
    }
    fn collect_online(&self, _dir: &Path, now: i64) -> Option<Result<Collected, ProviderError>> {
        Some(fetch(&OPENROUTER, "https://openrouter.ai/api/v1/key").and_then(|v| parse_openrouter(&v, now).ok_or_else(bad_response)))
    }
}

pub struct DeepSeek;

static DEEPSEEK: Info = Info {
    id: "deepseek",
    name: "DeepSeek",
    vendor: "DeepSeek",
    color: "#4d6bfe",
    hint: "Set DEEPSEEK_API_KEY in your environment, then turn this on. Shows your remaining prepaid balance.",
    endpoint: Some("api.deepseek.com"),
    documented: true,
    key_env: Some("DEEPSEEK_API_KEY"),
    default_enabled: false,
};

/// `GET /user/balance`. Balances arrive as decimal strings, one entry per currency.
pub fn parse_deepseek(v: &Value, now: i64) -> Option<Collected> {
    let metrics: Vec<Metric> = v
        .get("balance_infos")?
        .as_array()?
        .iter()
        .filter_map(|b| {
            let unit = match b.get("currency")?.as_str()? {
                "USD" => Unit::Usd,
                "CNY" => Unit::Cny,
                _ => return None,
            };
            let total: f64 = b.get("total_balance")?.as_str()?.parse().ok().filter(|n: &f64| n.is_finite())?;
            let id = format!("balance_{}", clean(b.get("currency")?.as_str()?).to_lowercase());
            Some(Metric {
                remaining: Some(total),
                qualification: Some("prepaid balance, not a rate limit".into()),
                ..Metric::new(&id, "balance", Category::ProviderQuota, unit, Source::ProviderApi, now)
            })
        })
        .collect();
    let mut notes = Vec::new();
    if v.get("is_available").and_then(Value::as_bool) == Some(false) {
        notes.push("balance too low for API calls".into());
    }
    (!metrics.is_empty()).then_some(Collected { metrics, notes, account: None })
}

impl Provider for DeepSeek {
    fn info(&self) -> &'static Info {
        &DEEPSEEK
    }
    fn detect(&self, _dir: &Path) -> bool {
        key(&DEEPSEEK).is_some()
    }
    fn collect_online(&self, _dir: &Path, now: i64) -> Option<Result<Collected, ProviderError>> {
        Some(fetch(&DEEPSEEK, "https://api.deepseek.com/user/balance").and_then(|v| parse_deepseek(&v, now).ok_or_else(bad_response)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openrouter_documented_shape() {
        let v = serde_json::json!({"data":{"label":"my-key","limit":20.0,"limit_reset":"monthly","limit_remaining":12.5,
            "usage":40.0,"usage_daily":0.75,"usage_weekly":3.0,"usage_monthly":7.5,"is_free_tier":false,
            "effective_budget":{"scope":"api_key","limit_usd":20.0,"spend_usd":7.5,"remaining_usd":12.5,"reset_interval":"monthly","resets_at":"2026-12-01T00:00:00Z"},
            "free_model_daily_requests":{"used":3,"limit":50,"remaining":47}}});
        let c = parse_openrouter(&v, 1).unwrap();
        let m = |id: &str| c.metrics.iter().find(|m| m.id == id).unwrap();
        assert_eq!((m("credits_day").used, m("credits_day").category), (Some(0.75), Category::ApiConsumption));
        assert_eq!((m("key_limit").total, m("key_limit").remaining, m("key_limit").used), (Some(20.0), Some(12.5), Some(7.5)));
        assert_eq!(m("budget").resets_at, parse_ts("2026-12-01T00:00:00Z"));
        assert_eq!((m("free_requests").remaining, m("free_requests").unit), (Some(47.0), Unit::Requests));
        assert!(c.notes.is_empty());
    }

    #[test]
    fn openrouter_uncapped_key_reports_no_invented_limit() {
        let v =
            serde_json::json!({"data":{"label":"k","limit":null,"limit_remaining":null,"usage":0,"usage_daily":0,"usage_monthly":0,"effective_budget":null}});
        let c = parse_openrouter(&v, 1).unwrap();
        assert!(c.metrics.iter().all(|m| m.category == Category::ApiConsumption && m.total.is_none()));
        assert_eq!(c.notes, ["no credit limit on this key"]);
        assert!(parse_openrouter(&serde_json::json!({"error":{"code":401}}), 0).is_none());
        assert!(parse_openrouter(&serde_json::json!({"data":{}}), 0).is_none());
    }

    #[test]
    fn deepseek_documented_shape() {
        let v = serde_json::json!({"is_available":true,"balance_infos":[
            {"currency":"CNY","total_balance":"110.00","granted_balance":"10.00","topped_up_balance":"100.00"},
            {"currency":"USD","total_balance":"3.50"},{"currency":"XXX","total_balance":"1"},{"currency":"USD","total_balance":"NaN"}]});
        let c = parse_deepseek(&v, 1).unwrap();
        assert_eq!(c.metrics.len(), 2);
        assert_eq!((c.metrics[0].id.as_str(), c.metrics[0].remaining, c.metrics[0].unit), ("balance_cny", Some(110.0), Unit::Cny));
        assert_eq!((c.metrics[1].remaining, c.metrics[1].total), (Some(3.5), None));
        let low = serde_json::json!({"is_available":false,"balance_infos":[{"currency":"USD","total_balance":"0.00"}]});
        assert_eq!(parse_deepseek(&low, 0).unwrap().notes, ["balance too low for API calls"]);
        assert!(parse_deepseek(&serde_json::json!({"balance_infos":[]}), 0).is_none());
    }
}
