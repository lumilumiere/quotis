use super::cursor::open_ro;
use super::{Info, Provider};
use crate::model::{Category, Collected, ErrorKind, Metric, ProviderError, Source, Unit};
use crate::settings::home;
use std::path::{Path, PathBuf};

pub struct OpenCode;

static INFO: Info = Info {
    id: "opencode",
    name: "opencode",
    vendor: "opencode",
    color: "#fab283",
    hint:
        "Shows tokens and estimated cost in the last 5 and 24 hours from opencode's local database. opencode uses your own API keys, so there is no plan limit.",
    endpoint: None,
    documented: true,
    key_env: None,
    default_enabled: true,
};

fn db(dir: &Path) -> PathBuf {
    dir.join("opencode.db")
}

impl Provider for OpenCode {
    fn info(&self) -> &'static Info {
        &INFO
    }

    fn default_dir(&self) -> PathBuf {
        home().join(".local/share/opencode")
    }

    fn detect(&self, dir: &Path) -> bool {
        db(dir).is_file()
    }

    fn watch_dir(&self, dir: &Path) -> Option<PathBuf> {
        Some(dir.to_path_buf())
    }

    fn collect_local(&self, dir: &Path, now: i64) -> Option<Result<Collected, ProviderError>> {
        if !db(dir).is_file() {
            return None;
        }
        let unreadable = || ProviderError::new(ErrorKind::Unavailable, "can't read opencode.db");
        let Some(c) = open_ro(&db(dir)) else { return Some(Err(unreadable())) };
        let since = |secs: i64| {
            c.query_row(
                "SELECT COALESCE(SUM(COALESCE(json_extract(data,'$.tokens.input'),0) + COALESCE(json_extract(data,'$.tokens.output'),0)
                         + COALESCE(json_extract(data,'$.tokens.reasoning'),0)),0),
                        COALESCE(SUM(json_extract(data,'$.cost')),0)
                 FROM message WHERE time_created >= ?1 AND time_created <= ?2 AND json_extract(data,'$.role') = 'assistant'",
                [(now - secs) * 1000, (now + 300) * 1000],
                |r| Ok((r.get::<_, i64>(0)?, r.get::<_, f64>(1)?)),
            )
        };
        let mut metrics = Vec::new();
        for (label, secs) in [("5h", 5 * 3600), ("24h", 24 * 3600)] {
            let Ok((tokens, cost)) = since(secs) else { return Some(Err(unreadable())) };
            let m = |id: &str, cat, unit, v: f64| {
                Metric { used: Some(v.max(0.0)), ..Metric::new(&format!("{id}_{label}"), label, cat, unit, Source::LocalDb, now) }.window(Some(secs as u64))
            };
            metrics.push(m("tokens", Category::LocalUsage, Unit::Tokens, tokens as f64));
            let mut cost = m("cost", Category::EstimatedCost, Unit::Usd, cost);
            cost.qualification = Some("opencode's own estimate from model prices, not a bill".into());
            metrics.push(cost);
        }
        Some(Ok(Collected { metrics, ..Default::default() }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sums_recent_assistant_messages() {
        let dir = crate::fsx::temp_dir("opencode");
        assert!(OpenCode.collect_local(&dir, 0).is_none());
        let now = 1_790_000_000;
        let c = rusqlite::Connection::open(db(&dir)).unwrap();
        c.execute_batch("CREATE TABLE message (id TEXT, session_id TEXT, time_created INTEGER, time_updated INTEGER, data TEXT)").unwrap();
        let add = |ago: i64, data: &str| c.execute("INSERT INTO message VALUES ('m','s',?1,?1,?2)", rusqlite::params![(now - ago) * 1000, data]).unwrap();
        add(60, r#"{"role":"assistant","cost":0.5,"tokens":{"input":100,"output":50,"reasoning":10,"cache":{"read":9999}}}"#);
        add(6 * 3600, r#"{"role":"assistant","cost":1.25,"tokens":{"input":1000,"output":0}}"#);
        add(30 * 3600, r#"{"role":"assistant","cost":9.0,"tokens":{"input":5000,"output":5000}}"#);
        add(60, r#"{"role":"user"}"#);
        // Dated a day ahead: a bad clock, not usage in the last five hours.
        add(-86_400, r#"{"role":"assistant","cost":3.0,"tokens":{"input":999}}"#);
        drop(c);
        let got = OpenCode.collect_local(&dir, now).unwrap().unwrap();
        let v = |id: &str| got.metrics.iter().find(|m| m.id == id).unwrap();
        assert_eq!((v("tokens_5h").used, v("tokens_24h").used), (Some(160.0), Some(1160.0)));
        assert_eq!((v("cost_5h").used, v("cost_24h").used), (Some(0.5), Some(1.75)));
        assert_eq!((v("cost_5h").category, v("tokens_5h").category), (Category::EstimatedCost, Category::LocalUsage));
        assert!(v("cost_24h").qualification.is_some());
    }

    #[test]
    fn a_database_with_another_schema_is_an_error_not_zeroes() {
        let dir = crate::fsx::temp_dir("opencode-bad");
        rusqlite::Connection::open(db(&dir)).unwrap().execute_batch("CREATE TABLE other (x)").unwrap();
        assert_eq!(OpenCode.collect_local(&dir, 0).unwrap().unwrap_err().kind, ErrorKind::Unavailable);
        std::fs::write(db(&dir), "this is not sqlite").unwrap();
        assert!(OpenCode.collect_local(&dir, 0).unwrap().is_err());
    }
}
