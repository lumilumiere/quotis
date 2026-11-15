//! The normalized usage contract shared by the desktop app, the CLI and the agent skill.
//! Bump `SCHEMA_VERSION` on any breaking change to these types.

use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION: u32 = 1;

/// What kind of number a metric is. These never convert into each other:
/// a local token count is not a quota, and a cost is an estimate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    /// An allowance the provider itself reported (percent, credits, balance).
    ProviderQuota,
    /// Usage Quotis measured from the tool's local files.
    LocalUsage,
    /// Consumption the provider's API reported, with no allowance attached.
    ApiConsumption,
    /// A money figure derived from token prices, not a bill.
    EstimatedCost,
    /// The provider does not expose this metric.
    Unavailable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Unit {
    Percent,
    Tokens,
    Requests,
    Usd,
    Cny,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    ProviderApi,
    LocalLog,
    LocalDb,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Freshness {
    Fresh,
    /// Older than the provider's refresh budget; the number may have moved.
    Stale,
    /// The reset time passed after this was observed; the number is no longer true.
    Expired,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Metric {
    pub id: String,
    pub label: String,
    pub category: Category,
    pub unit: Unit,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub window_seconds: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub used: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub remaining: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub total: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub resets_at: Option<i64>,
    pub observed_at: i64,
    pub source: Source,
    pub freshness: Freshness,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub qualification: Option<String>,
}

impl Metric {
    pub fn new(id: &str, label: &str, category: Category, unit: Unit, source: Source, observed_at: i64) -> Self {
        Metric {
            id: id.into(),
            label: label.into(),
            category,
            unit,
            window_seconds: None,
            used: None,
            remaining: None,
            total: None,
            resets_at: None,
            observed_at,
            source,
            freshness: Freshness::Fresh,
            qualification: None,
        }
    }

    /// A provider-reported percentage of an allowance.
    pub fn quota_percent(id: &str, label: &str, used: f64, resets_at: Option<i64>, source: Source, observed_at: i64) -> Self {
        let used = used.clamp(0.0, 100.0);
        Metric {
            used: Some(used),
            remaining: Some(100.0 - used),
            total: Some(100.0),
            resets_at: resets_at.filter(|&t| t > 0),
            ..Metric::new(id, label, Category::ProviderQuota, Unit::Percent, source, observed_at)
        }
    }

    pub fn window(mut self, secs: Option<u64>) -> Self {
        self.window_seconds = secs.filter(|&s| s > 0);
        self
    }

    /// Recomputes freshness for `now`. `stale_after` is the provider's refresh budget in seconds.
    pub fn refresh_freshness(&mut self, now: i64, stale_after: i64) {
        let reset_passed = self.resets_at.is_some_and(|r| r <= now && self.observed_at < r);
        // An observation from the future means the clock moved; its age cannot be trusted.
        let skewed = self.observed_at > now + 300;
        self.freshness = if reset_passed {
            Freshness::Expired
        } else if skewed || now - self.observed_at > stale_after {
            Freshness::Stale
        } else {
            Freshness::Fresh
        };
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Connected,
    /// Enabled, but the tool's files or login were not found.
    NotDetected,
    Disabled,
    /// Detected, but the last collection failed and there is no data to show.
    Error,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    NotSignedIn,
    AuthExpired,
    RateLimited,
    Offline,
    Unavailable,
    BadResponse,
    PermissionDenied,
    Internal,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProviderError {
    pub kind: ErrorKind,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub retry_after_secs: Option<u64>,
}

impl ProviderError {
    pub fn new(kind: ErrorKind, message: &str) -> Self {
        ProviderError { kind, message: clean(message), retry_after_secs: None }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProviderSnapshot {
    pub id: String,
    pub name: String,
    pub vendor: String,
    /// Non-identifying account context the provider reported, such as a plan name.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub account: Option<String>,
    pub status: Status,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub error: Option<ProviderError>,
    /// Last successful collection, unix seconds.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub observed_at: Option<i64>,
    pub refreshing: bool,
    pub metrics: Vec<Metric>,
    #[serde(default)]
    pub notes: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub schema_version: u32,
    pub generated_at: i64,
    pub app_version: String,
    /// How old an observation may be, in seconds, before it counts as stale. Lets a reader
    /// re-judge freshness later without knowing the app's settings.
    #[serde(default = "default_stale_after")]
    pub stale_after_secs: i64,
    pub providers: Vec<ProviderSnapshot>,
}

fn default_stale_after() -> i64 {
    900
}

impl Snapshot {
    /// Re-judges every metric's freshness for `now`. A snapshot read from disk was fresh
    /// when written; a window may have reset, or the numbers aged, since then.
    pub fn refresh(&mut self, now: i64) {
        let budget = self.stale_after_secs.max(60);
        for m in self.providers.iter_mut().flat_map(|p| p.metrics.iter_mut()) {
            m.refresh_freshness(now, budget);
        }
    }
}

/// What one collection pass produced.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Collected {
    pub metrics: Vec<Metric>,
    pub notes: Vec<String>,
    pub account: Option<String>,
}

/// Makes a provider-derived string safe to store and show: no control characters, bounded length.
pub fn clean(s: &str) -> String {
    s.chars().filter(|c| !c.is_control()).take(120).collect::<String>().trim().to_string()
}

/// A short label for a quota window, derived only from a duration the provider reported.
pub fn window_label(secs: u64) -> String {
    match secs {
        18_000 => "5h".into(),
        86_400 => "day".into(),
        604_800 => "wk".into(),
        2_419_200..=2_678_400 => "mo".into(),
        s if s % 86_400 == 0 => format!("{}d", s / 86_400),
        s if s % 3_600 == 0 => format!("{}h", s / 3_600),
        s => format!("{}m", s / 60),
    }
}
