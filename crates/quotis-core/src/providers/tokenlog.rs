//! Gemini CLI and Qwen Code write per-message token counts to JSONL chat logs.
//! These are locally measured usage: there is no allowance in the files, so no quota is shown.

use super::{Info, Provider};
use crate::fsx::{jsonl_files, read_new_lines};
use crate::model::{Category, Collected, Metric, ProviderError, Source, Unit};
use crate::settings::home;
use crate::time::parse_ts;
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

const DAY: i64 = 24 * 3600;
/// A record dated further ahead than this is treated as a bad clock, not as usage.
const FUTURE_SKEW: i64 = 300;
/// Upper bound on log files tracked at once.
const MAX_FILES: usize = 2000;

#[derive(Default)]
struct FileState {
    offset: u64,
    entries: Vec<(i64, u64)>,
}

pub struct TokenLog {
    info: &'static Info,
    folder: &'static str,
    /// Per-file read checkpoints, so each pass parses only newly appended lines.
    files: Mutex<(PathBuf, HashMap<PathBuf, FileState>)>,
}

static GEMINI: Info = Info {
    id: "gemini",
    name: "Gemini CLI",
    vendor: "Google",
    color: "#4285f4",
    hint: "Shows tokens used in the last 5 and 24 hours, measured from Gemini CLI's local logs. Gemini does not store its quota locally.",
    endpoint: None,
    documented: true,
    key_env: None,
    default_enabled: true,
};

static QWEN: Info = Info {
    id: "qwen",
    name: "Qwen Code",
    vendor: "Alibaba",
    color: "#615ced",
    hint: "Shows tokens used in the last 5 and 24 hours, measured from Qwen Code's local logs.",
    endpoint: None,
    documented: true,
    key_env: None,
    default_enabled: true,
};

impl TokenLog {
    pub fn gemini() -> Self {
        TokenLog { info: &GEMINI, folder: ".gemini", files: Default::default() }
    }
    pub fn qwen() -> Self {
        TokenLog { info: &QWEN, folder: ".qwen", files: Default::default() }
    }
}

fn logs(dir: &Path) -> PathBuf {
    dir.join("tmp")
}

pub fn parse_lines(text: &str) -> Vec<(i64, u64)> {
    text.lines()
        .filter(|l| l.contains("\"tokens\""))
        .filter_map(|l| {
            let v: Value = serde_json::from_str(l).ok()?;
            Some((parse_ts(v.get("timestamp")?.as_str()?)?, v.pointer("/tokens/total")?.as_u64()?))
        })
        .collect()
}

fn usage_metrics(entries: impl Iterator<Item = (i64, u64)> + Clone, now: i64) -> Vec<Metric> {
    [("tokens_5h", "5h", 5 * 3600), ("tokens_24h", "24h", DAY)]
        .into_iter()
        .map(|(id, label, secs)| {
            let sum: u64 = entries.clone().filter(|(ts, _)| (-FUTURE_SKEW..secs).contains(&(now - ts))).map(|(_, n)| n).sum();
            Metric { used: Some(sum as f64), ..Metric::new(id, label, Category::LocalUsage, Unit::Tokens, Source::LocalLog, now) }.window(Some(secs as u64))
        })
        .collect()
}

impl Provider for TokenLog {
    fn info(&self) -> &'static Info {
        self.info
    }

    fn default_dir(&self) -> PathBuf {
        home().join(self.folder)
    }

    fn detect(&self, dir: &Path) -> bool {
        logs(dir).is_dir()
    }

    fn watch_dir(&self, dir: &Path) -> Option<PathBuf> {
        Some(logs(dir))
    }

    fn collect_local(&self, dir: &Path, now: i64) -> Option<Result<Collected, ProviderError>> {
        if !logs(dir).is_dir() {
            return None;
        }
        let mut guard = self.files.lock().unwrap_or_else(|e| e.into_inner());
        let (root, files) = &mut *guard;
        if root != dir {
            *root = dir.to_path_buf();
            files.clear();
        }
        let cutoff = SystemTime::now() - Duration::from_secs(DAY as u64);
        // ponytail: one metadata walk per pass; contents are read incrementally. Switch to the
        // watcher's changed-path list if a log folder ever grows past MAX_FILES.
        let recent: Vec<PathBuf> = jsonl_files(&logs(dir), MAX_FILES).into_iter().filter(|(_, m)| *m > cutoff).map(|(p, _)| p).collect();
        files.retain(|p, _| recent.contains(p));
        for p in recent {
            let st = files.entry(p.clone()).or_default();
            let before = st.offset;
            if let Ok(text) = read_new_lines(&p, &mut st.offset) {
                if st.offset < before {
                    st.entries.clear();
                }
                st.entries.extend(parse_lines(&text));
            }
            st.entries.retain(|(ts, _)| now - ts < DAY);
        }
        let metrics = usage_metrics(files.values().flat_map(|f| f.entries.iter().copied()), now);
        Some(Ok(Collected { metrics, ..Default::default() }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn line(ts: &str, total: u64) -> String {
        format!("{{\"timestamp\":\"{ts}\",\"type\":\"gemini\",\"content\":\"private prompt text\",\"tokens\":{{\"input\":1,\"total\":{total}}}}}\n")
    }

    #[test]
    fn parses_only_token_lines() {
        let text = format!(
            "{}{}{{\"timestamp\":\"x\",\"tokens\":{{\"total\":5}}}}\nnot json \"tokens\"\n{{\"type\":\"user\"}}\n",
            line("2026-09-22T09:00:00Z", 10),
            line("2026-09-22T11:00:00+02:00", 7)
        );
        let e = parse_lines(&text);
        assert_eq!(e, [(1_790_067_600, 10), (1_790_067_600, 7)]);
    }

    #[test]
    fn future_dated_records_are_not_counted_as_recent_usage() {
        let now = 1_000_000;
        let m = usage_metrics([(now + 86_400, 999), (now + 60, 5), (now - 60, 100)].into_iter(), now);
        assert_eq!((m[0].used, m[1].used), (Some(105.0), Some(105.0)), "small clock skew is tolerated, a day ahead is not");
    }

    #[test]
    fn windows_sum_and_stay_local_usage() {
        let now = 1_000_000;
        let m = usage_metrics([(now - 60, 100), (now - 6 * 3600, 1000), (now - 30 * 3600, 5000)].into_iter(), now);
        assert_eq!((m[0].used, m[1].used), (Some(100.0), Some(1100.0)));
        assert!(m.iter().all(|m| m.category == Category::LocalUsage && m.total.is_none() && m.remaining.is_none()));
    }

    #[test]
    fn incremental_across_appends_truncation_and_folder_change() {
        let dir = crate::fsx::temp_dir("tokenlog");
        let p = TokenLog::gemini();
        assert!(p.collect_local(&dir, 0).is_none());
        std::fs::create_dir_all(dir.join("tmp/proj/chats")).unwrap();
        let file = dir.join("tmp/proj/chats/s.jsonl");
        let now = parse_ts("2026-09-22T10:00:00Z").unwrap();
        std::fs::write(&file, line("2026-09-22T09:00:00Z", 10)).unwrap();
        let used = |c: Collected| c.metrics[1].used.unwrap();
        assert_eq!(used(p.collect_local(&dir, now).unwrap().unwrap()), 10.0);
        // Unchanged file: nothing is re-read, nothing double counts.
        assert_eq!(used(p.collect_local(&dir, now).unwrap().unwrap()), 10.0);
        std::fs::OpenOptions::new().append(true).open(&file).unwrap().write_all(line("2026-09-22T09:30:00Z", 5).as_bytes()).unwrap();
        assert_eq!(used(p.collect_local(&dir, now).unwrap().unwrap()), 15.0);
        // Truncated and rewritten: old entries are dropped, not added to.
        std::fs::write(&file, line("2026-09-22T09:45:00Z", 3)).unwrap();
        assert_eq!(used(p.collect_local(&dir, now).unwrap().unwrap()), 3.0);
        // A different folder starts from scratch.
        let other = crate::fsx::temp_dir("tokenlog2");
        std::fs::create_dir_all(other.join("tmp")).unwrap();
        assert_eq!(used(p.collect_local(&other, now).unwrap().unwrap()), 0.0);
    }

    #[test]
    fn entries_age_out() {
        let dir = crate::fsx::temp_dir("tokenlog-age");
        std::fs::create_dir_all(dir.join("tmp")).unwrap();
        std::fs::write(dir.join("tmp/s.jsonl"), line("2026-09-22T09:00:00Z", 10)).unwrap();
        let p = TokenLog::qwen();
        let t = parse_ts("2026-09-22T10:00:00Z").unwrap();
        assert_eq!(p.collect_local(&dir, t).unwrap().unwrap().metrics[0].used, Some(10.0));
        assert_eq!(p.collect_local(&dir, t + 6 * 3600).unwrap().unwrap().metrics[0].used, Some(0.0));
        assert_eq!(p.collect_local(&dir, t + 25 * 3600).unwrap().unwrap().metrics[1].used, Some(0.0));
    }
}
