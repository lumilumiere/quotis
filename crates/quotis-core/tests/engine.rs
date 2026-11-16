//! Engine behaviour against scripted providers: scheduling, backoff, merging, staleness,
//! fault isolation and concurrent updates. No network, no real provider files.

use quotis_core::providers::{Info, Provider};
use quotis_core::*;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

static FAKE: Info = Info {
    id: "fake",
    name: "Fake",
    vendor: "Test",
    color: "#fff",
    hint: "",
    endpoint: Some("example.invalid"),
    documented: true,
    key_env: None,
    default_enabled: true,
};

static OTHER: Info = Info { id: "other", name: "Other", endpoint: None, ..FAKE };

type Script = Mutex<Vec<Result<Collected, ProviderError>>>;

/// Returns scripted online results in order; panics when told to.
struct Fake {
    info: &'static Info,
    online: Script,
    local: Script,
    panic_online: bool,
    /// How many times detection touched this provider.
    detects: AtomicUsize,
}

impl Fake {
    fn new(info: &'static Info) -> Arc<Self> {
        Arc::new(Fake { info, online: Default::default(), local: Default::default(), panic_online: false, detects: AtomicUsize::new(0) })
    }
}

impl Provider for Fake {
    fn info(&self) -> &'static Info {
        self.info
    }
    fn default_dir(&self) -> PathBuf {
        PathBuf::from("/fake/default")
    }
    fn detect(&self, _dir: &Path) -> bool {
        self.detects.fetch_add(1, Ordering::Relaxed);
        true
    }
    fn watch_dir(&self, dir: &Path) -> Option<PathBuf> {
        Some(dir.join("logs"))
    }
    fn collect_local(&self, _dir: &Path, _now: i64) -> Outcome {
        let mut s = self.local.lock().unwrap();
        (!s.is_empty()).then(|| s.remove(0))
    }
    fn collect_online(&self, _dir: &Path, _now: i64) -> Outcome {
        assert!(!self.panic_online, "scripted panic");
        let mut s = self.online.lock().unwrap();
        Some(if s.is_empty() { Err(ProviderError::new(ErrorKind::Offline, "offline")) } else { s.remove(0) })
    }
}

fn quota(id: &str, used: f64, resets: Option<i64>, source: Source, observed: i64) -> Collected {
    Collected { metrics: vec![Metric::quota_percent(id, "5h", used, resets, source, observed)], ..Default::default() }
}

fn err(kind: ErrorKind) -> ProviderError {
    ProviderError::new(kind, "scripted")
}

fn engine(fake: Arc<Fake>) -> Engine {
    let mut e = Engine::with_providers(Settings::default(), vec![fake]);
    e.probe_now();
    e
}

/// Runs one local collection and records it at `now`.
fn local(e: &mut Engine, now: i64) {
    let j = e.local_jobs().remove(0);
    let out = j.run(now);
    e.finish(&j, out, now);
}

/// Runs every due online job once and records it.
fn poll(e: &mut Engine, now: i64) -> usize {
    let jobs = e.due_online(now);
    for j in &jobs {
        let out = j.run(now);
        e.finish(j, out, now);
    }
    jobs.len()
}

fn only(e: &Engine, now: i64) -> ProviderSnapshot {
    e.snapshot(now).providers.remove(0)
}

#[test]
fn first_poll_is_due_then_waits_for_the_interval() {
    let fake = Fake::new(&FAKE);
    fake.online.lock().unwrap().push(Ok(quota("q", 40.0, None, Source::ProviderApi, 1000)));
    let mut e = engine(fake);
    assert_eq!(only(&e, 1000).status, Status::Connected, "detected, awaiting data");
    assert_eq!(poll(&mut e, 1000), 1);
    assert_eq!(poll(&mut e, 1001), 0, "not due again right away");
    let p = only(&e, 1001);
    assert_eq!((p.status, p.metrics[0].remaining, p.metrics[0].freshness), (Status::Connected, Some(60.0), Freshness::Fresh));
    assert!(p.error.is_none() && !p.refreshing);
    // After the refresh interval (5 min default, plus up to 10% jitter) it is due again.
    e.age_last_poll("fake", Duration::from_secs(331));
    assert_eq!(e.due_online(1331).len(), 1);
    assert!(only(&e, 1331).refreshing, "a poll in flight is visible");
    assert_eq!(e.due_online(1331).len(), 0, "never two polls in flight for one provider");
}

#[test]
fn rate_limit_backs_off_and_keeps_last_numbers() {
    let fake = Fake::new(&FAKE);
    fake.online.lock().unwrap().extend([
        Ok(quota("q", 40.0, None, Source::ProviderApi, 0)),
        Err(ProviderError { retry_after_secs: Some(900), ..err(ErrorKind::RateLimited) }),
        Err(err(ErrorKind::RateLimited)),
        Err(ProviderError { retry_after_secs: Some(999_999), ..err(ErrorKind::RateLimited) }),
    ]);
    let mut e = engine(fake);
    poll(&mut e, 0);
    e.age_last_poll("fake", Duration::from_secs(400));
    poll(&mut e, 400);
    assert_eq!(e.backoff_secs("fake"), 900, "Retry-After is honoured");
    let p = only(&e, 400);
    assert_eq!((p.status, p.metrics.len()), (Status::Connected, 1), "last numbers stay visible");
    assert_eq!(p.error.unwrap().kind, ErrorKind::RateLimited);
    e.age_last_poll("fake", Duration::from_secs(400));
    assert_eq!(poll(&mut e, 800), 0, "still inside the backoff window");
    e.age_last_poll("fake", Duration::from_secs(1000));
    poll(&mut e, 1800);
    assert_eq!(e.backoff_secs("fake"), 1800, "no Retry-After: exponential");
    e.age_last_poll("fake", Duration::from_secs(4000));
    poll(&mut e, 5800);
    assert_eq!(e.backoff_secs("fake"), 999999, "a server's Retry-After is never shortened");
}

#[test]
fn repeated_failures_back_off_exponentially_then_recover() {
    let fake = Fake::new(&FAKE);
    fake.online.lock().unwrap().extend([
        Err(err(ErrorKind::Offline)),
        Err(err(ErrorKind::Unavailable)),
        Err(err(ErrorKind::BadResponse)),
        Ok(quota("q", 1.0, None, Source::ProviderApi, 0)),
    ]);
    let mut e = engine(fake);
    let mut backoffs = Vec::new();
    for _ in 0..3 {
        e.age_last_poll("fake", Duration::from_secs(4000));
        poll(&mut e, 0);
        backoffs.push(e.backoff_secs("fake"));
    }
    assert_eq!(backoffs, [300, 600, 1200]);
    let p = only(&e, 0);
    assert_eq!((p.status, p.error.as_ref().unwrap().kind), (Status::Error, ErrorKind::BadResponse), "no data and an error is an error state");
    e.age_last_poll("fake", Duration::from_secs(4000));
    poll(&mut e, 0);
    assert_eq!(e.backoff_secs("fake"), 0);
    assert!(only(&e, 0).error.is_none());
}

#[test]
fn an_expired_window_is_qualified_and_repolled_early() {
    let fake = Fake::new(&FAKE);
    fake.online
        .lock()
        .unwrap()
        .extend([Ok(quota("q", 90.0, Some(2000), Source::ProviderApi, 1500)), Ok(quota("q", 0.0, Some(20_000), Source::ProviderApi, 2200))]);
    let mut e = engine(fake);
    poll(&mut e, 1500);
    assert_eq!(only(&e, 1999).metrics[0].freshness, Freshness::Fresh);
    let m = only(&e, 2000).metrics.remove(0);
    // The old 10% is still reported, but flagged: it is never silently turned into "100% left".
    assert_eq!((m.freshness, m.remaining), (Freshness::Expired, Some(10.0)));
    e.age_last_poll("fake", Duration::from_secs(60));
    assert_eq!(poll(&mut e, 2060), 0, "eager re-poll still waits two minutes");
    e.age_last_poll("fake", Duration::from_secs(125));
    assert_eq!(poll(&mut e, 2200), 1, "a passed reset triggers a re-poll well before the 5 minute interval");
    let m = only(&e, 2200).metrics.remove(0);
    assert_eq!((m.freshness, m.remaining), (Freshness::Fresh, Some(100.0)));
}

#[test]
fn old_observations_go_stale_and_clock_skew_is_not_trusted() {
    let fake = Fake::new(&FAKE);
    fake.online.lock().unwrap().push(Ok(quota("q", 50.0, None, Source::ProviderApi, 10_000)));
    let mut e = engine(fake);
    poll(&mut e, 10_000);
    assert_eq!(only(&e, 10_000 + 900).metrics[0].freshness, Freshness::Fresh);
    assert_eq!(only(&e, 10_000 + 901).metrics[0].freshness, Freshness::Stale, "three refresh intervals without an update");
    assert_eq!(only(&e, 10_000 - 3600).metrics[0].freshness, Freshness::Stale, "clock moved backwards");
}

#[test]
fn local_and_online_sources_merge_by_newest_observation() {
    let fake = Fake::new(&FAKE);
    fake.local.lock().unwrap().extend([
        Ok(quota("q", 10.0, None, Source::LocalLog, 500)),
        Ok(quota("q", 30.0, None, Source::LocalLog, 1500)),
        Ok(quota("q", 99.0, None, Source::LocalLog, 100)),
    ]);
    fake.online.lock().unwrap().push(Ok(quota("q", 20.0, None, Source::ProviderApi, 1000)));
    let mut e = engine(fake);
    local(&mut e, 600);
    assert_eq!(only(&e, 600).metrics[0].used, Some(10.0));
    poll(&mut e, 1000);
    assert_eq!(only(&e, 1000).metrics[0].used, Some(20.0), "newer online value replaces the older log value");
    local(&mut e, 1500);
    assert_eq!(only(&e, 1500).metrics[0].used, Some(30.0), "a newer log line replaces the online value");
    local(&mut e, 1500);
    assert_eq!(only(&e, 1500).metrics[0].used, Some(30.0), "an older log line never overwrites newer data");
    assert_eq!(only(&e, 1500).metrics.len(), 1);
}

#[test]
fn signed_out_online_is_quiet_when_local_data_exists() {
    let fake = Fake::new(&FAKE);
    fake.local.lock().unwrap().push(Ok(quota("q", 10.0, None, Source::LocalLog, 0)));
    fake.online.lock().unwrap().extend([Err(err(ErrorKind::NotSignedIn)), Err(err(ErrorKind::AuthExpired))]);
    let mut e = engine(fake);
    local(&mut e, 0);
    poll(&mut e, 0);
    let p = only(&e, 0);
    assert!(p.error.is_none() && p.status == Status::Connected);
    e.age_last_poll("fake", Duration::from_secs(4000));
    poll(&mut e, 0);
    assert_eq!(only(&e, 0).error.unwrap().kind, ErrorKind::AuthExpired, "an expired login is always reported");
}

#[test]
fn auth_loss_drops_api_numbers() {
    let fake = Fake::new(&FAKE);
    fake.online.lock().unwrap().extend([Ok(quota("q", 40.0, None, Source::ProviderApi, 0)), Err(err(ErrorKind::NotSignedIn))]);
    let mut e = engine(fake);
    poll(&mut e, 0);
    e.age_last_poll("fake", Duration::from_secs(4000));
    poll(&mut e, 0);
    let p = only(&e, 0);
    assert_eq!((p.status, p.metrics.len()), (Status::NotDetected, 0));
}

#[test]
fn a_panicking_provider_does_not_take_the_others_down() {
    let bad = Arc::new(Fake { info: &FAKE, online: Default::default(), local: Default::default(), panic_online: true, detects: AtomicUsize::new(0) });
    let good = Fake::new(&OTHER);
    good.local.lock().unwrap().push(Ok(quota("q", 5.0, None, Source::LocalLog, 0)));
    let mut e = Engine::with_providers(Settings::default(), vec![bad, good]);
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let snap = e.collect_once(true);
    std::panic::set_hook(prev);
    assert_eq!(snap.providers[0].error.as_ref().unwrap().kind, ErrorKind::Internal);
    assert_eq!(snap.providers[1].metrics[0].used, Some(5.0));
}

#[test]
fn results_for_a_changed_or_disabled_provider_are_dropped() {
    let fake = Fake::new(&FAKE);
    fake.online.lock().unwrap().extend([Ok(quota("q", 40.0, None, Source::ProviderApi, 0)), Ok(quota("q", 41.0, None, Source::ProviderApi, 0))]);
    let mut e = engine(fake);
    let job = e.due_online(0).remove(0);
    let out = job.run(0);
    let mut s = Settings::default();
    let elsewhere = std::env::temp_dir().join("quotis-elsewhere");
    s.providers.insert("fake".into(), ProviderCfg { enabled: true, dir: elsewhere.display().to_string() });
    e.set_settings(s.clone());
    e.finish(&job, out, 0);
    assert!(only(&e, 0).metrics.is_empty(), "a result for the old folder must not land in the new one");
    assert_eq!(e.dir_of("fake"), Some(elsewhere.as_path()));

    let job = e.due_online(0).remove(0);
    let out = job.run(0);
    s.providers.get_mut("fake").unwrap().enabled = false;
    e.set_settings(s);
    e.finish(&job, out, 0);
    let p = only(&e, 0);
    assert_eq!((p.status, p.metrics.len(), p.refreshing), (Status::Disabled, 0, false));
    assert!(e.due_online(0).is_empty() && e.local_jobs().is_empty() && e.watch_dirs().is_empty());
}

#[test]
fn a_result_from_before_a_round_trip_reconfiguration_is_dropped() {
    // Folder A -> B -> A: the folder matches again, but the result predates two reconfigurations.
    let fake = Fake::new(&FAKE);
    fake.online.lock().unwrap().push(Ok(quota("q", 40.0, None, Source::ProviderApi, 0)));
    let mut e = engine(fake);
    let old_job = e.due_online(0).remove(0);
    let out = old_job.run(0);
    let mut b = Settings::default();
    b.providers.insert("fake".into(), ProviderCfg { enabled: true, dir: std::env::temp_dir().join("quotis-b").display().to_string() });
    e.set_settings(b);
    e.set_settings(Settings::default());
    assert_eq!(e.dir_of("fake"), Some(old_job.dir.as_path()), "back on the original folder");
    e.finish(&old_job, out, 0);
    assert!(only(&e, 0).metrics.is_empty(), "the stale result must not be accepted");
}

#[test]
fn disabled_providers_are_not_probed() {
    let mut s = Settings::default();
    s.providers.insert("fake".into(), ProviderCfg { enabled: false, dir: String::new() });
    let fake = Fake::new(&FAKE);
    let mut e = Engine::with_providers(s, vec![fake.clone()]);
    e.probe_now();
    let st = e.statuses().remove(0);
    assert!(!st.enabled && !st.found);
    let _ = e.collect_once(false);
    assert_eq!(fake.detects.load(Ordering::Relaxed), 0, "a disabled provider's files and login are never touched");
    // Turned on, it is probed.
    e.set_settings(Settings::default());
    assert_eq!(fake.detects.load(Ordering::Relaxed), 0, "applying settings does no I/O by itself");
    e.probe_now();
    assert_eq!(fake.detects.load(Ordering::Relaxed), 1);
    assert!(e.statuses()[0].found);
}

#[test]
fn file_changes_select_their_provider_and_trigger_an_early_poll() {
    let fake = Fake::new(&FAKE);
    fake.online.lock().unwrap().extend([Ok(quota("q", 1.0, None, Source::ProviderApi, 0)), Ok(quota("q", 2.0, None, Source::ProviderApi, 0))]);
    let mut e = engine(fake);
    poll(&mut e, 0);
    assert!(e.local_jobs_for(&[PathBuf::from("/unrelated/file.jsonl")]).is_empty());
    assert_eq!(e.local_jobs_for(&[PathBuf::from("/fake/default/logs/2026/s.jsonl")]).len(), 1);
    e.age_last_poll("fake", Duration::from_secs(60));
    assert_eq!(poll(&mut e, 0), 0, "activity re-polls after two minutes, not instantly");
    e.age_last_poll("fake", Duration::from_secs(121));
    assert_eq!(poll(&mut e, 0), 1);
    e.age_last_poll("fake", Duration::from_secs(121));
    assert_eq!(poll(&mut e, 0), 0, "the activity flag is consumed by the poll");
}

#[test]
fn concurrent_updates_from_many_threads_stay_consistent() {
    let fake = Fake::new(&FAKE);
    let e = Arc::new(Mutex::new(engine(fake.clone())));
    std::thread::scope(|s| {
        for t in 0..8i64 {
            let e = e.clone();
            s.spawn(move || {
                for i in 0..200i64 {
                    let observed = t * 1000 + i;
                    let out = Some(Ok(quota("q", (observed % 100) as f64, None, Source::LocalLog, observed)));
                    {
                        let mut engine = e.lock().unwrap();
                        let job = engine.local_jobs().remove(0);
                        engine.finish(&job, out, observed);
                    }
                    let snap = e.lock().unwrap().snapshot(observed);
                    assert!(snap.providers[0].metrics.len() <= 1);
                }
            });
        }
    });
    let p = only(&e.lock().unwrap(), 0);
    assert_eq!((p.metrics.len(), p.metrics[0].observed_at), (1, 7199), "the newest observation wins regardless of arrival order");
}

#[test]
fn snapshots_carry_no_secrets_and_survive_a_round_trip() {
    let fake = Fake::new(&FAKE);
    fake.online.lock().unwrap().push(Ok(Collected {
        account: Some("pro".into()),
        notes: vec!["note".into()],
        ..quota("q", 40.0, Some(99), Source::ProviderApi, 5)
    }));
    let mut e = engine(fake);
    poll(&mut e, 5);
    let snap = e.snapshot(6);
    let json = serde_json::to_string(&snap).unwrap();
    for forbidden in ["token", "secret", "password", "authorization", "cookie", "/fake/default"] {
        assert!(!json.to_lowercase().contains(forbidden), "snapshot JSON must not contain {forbidden:?}: {json}");
    }
    assert_eq!(cache::parse(&json).unwrap(), snap);
    assert_eq!(snap.schema_version, SCHEMA_VERSION);
}

#[test]
fn hostile_error_text_is_cleaned() {
    let e = ProviderError::new(ErrorKind::Unavailable, &format!("<script>alert(1)</script>\u{1b}[31m\n{}", "x".repeat(500)));
    assert!(e.message.chars().count() <= 120 && !e.message.chars().any(char::is_control));
}

#[test]
fn a_local_failure_after_success_is_reported_and_clears_on_recovery() {
    let fake = Fake::new(&FAKE);
    fake.local.lock().unwrap().extend([
        Ok(quota("q", 10.0, None, Source::LocalLog, 100)),
        Err(err(ErrorKind::PermissionDenied)),
        Ok(quota("q", 11.0, None, Source::LocalLog, 300)),
    ]);
    let mut e = engine(fake);
    local(&mut e, 100);
    local(&mut e, 200);
    let p = only(&e, 200);
    assert_eq!(p.error.as_ref().map(|e| e.kind), Some(ErrorKind::PermissionDenied), "the failure is shown, not swallowed");
    assert_eq!(p.metrics[0].used, Some(10.0), "last numbers are kept alongside it");
    local(&mut e, 300);
    let p = only(&e, 300);
    assert!(p.error.is_none());
    assert_eq!(p.metrics[0].used, Some(11.0));
}

#[test]
fn when_the_local_source_disappears_its_numbers_go_too() {
    let fake = Fake::new(&FAKE);
    fake.local.lock().unwrap().push(Ok(quota("q", 10.0, None, Source::LocalLog, 100)));
    fake.online.lock().unwrap().push(Ok(quota("api", 50.0, None, Source::ProviderApi, 100)));
    let mut e = engine(fake);
    local(&mut e, 100);
    poll(&mut e, 100);
    assert_eq!(only(&e, 100).metrics.len(), 2);
    // The script is now empty: the fake's local source returns None, as a removed folder would.
    local(&mut e, 200);
    let p = only(&e, 200);
    assert_eq!(p.metrics.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(), ["api"], "API numbers stay, local ones are dropped");
}

#[test]
fn a_local_pass_replaces_what_the_previous_local_pass_reported() {
    let fake = Fake::new(&FAKE);
    let two = Collected {
        metrics: vec![Metric::quota_percent("a", "5h", 1.0, None, Source::LocalLog, 100), Metric::quota_percent("b", "wk", 2.0, None, Source::LocalLog, 100)],
        ..Default::default()
    };
    fake.local.lock().unwrap().extend([Ok(two), Ok(quota("a", 3.0, None, Source::LocalLog, 200))]);
    let mut e = engine(fake);
    local(&mut e, 100);
    local(&mut e, 200);
    let p = only(&e, 200);
    assert_eq!(p.metrics.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(), ["a"], "a metric the source no longer reports is not left behind");
}

#[test]
fn a_future_dated_local_reading_is_never_believed_however_often_it_is_reread() {
    let fake = Fake::new(&FAKE);
    // A log line stamped in the year 2100, read again on every local pass.
    let bad = || Ok(quota("q", 99.0, None, Source::LocalLog, 4_102_444_800));
    fake.local.lock().unwrap().extend([bad(), bad(), Ok(quota("q", 30.0, None, Source::LocalLog, 1290))]);
    fake.online.lock().unwrap().push(Ok(quota("q", 20.0, None, Source::ProviderApi, 1100)));
    let mut e = engine(fake);
    local(&mut e, 1000);
    let p = only(&e, 1000);
    assert!(p.metrics.is_empty(), "not shown, and not re-dated to look current");
    assert_eq!(p.error.as_ref().map(|e| e.kind), Some(ErrorKind::BadResponse), "and the problem is visible");
    poll(&mut e, 1100);
    assert_eq!(only(&e, 1100).metrics[0].used, Some(20.0));
    // The same bad record again, after a real API reading: it must not displace it.
    local(&mut e, 1200);
    let p = only(&e, 1200);
    assert_eq!((p.metrics[0].used, p.metrics[0].observed_at, p.metrics[0].freshness), (Some(20.0), 1100, Freshness::Fresh));
    assert!(p.error.is_some(), "an online success does not hide a local problem");
    // A sane local reading is accepted again and clears the error. A few minutes of clock skew is fine.
    local(&mut e, 1300);
    let p = only(&e, 1300);
    assert_eq!((p.metrics[0].used, p.error.is_none()), (Some(30.0), true));
    let fake = Fake::new(&FAKE);
    fake.local.lock().unwrap().push(Ok(quota("q", 5.0, None, Source::LocalLog, 1000 + 200)));
    let mut e = engine(fake);
    local(&mut e, 1000);
    assert_eq!(only(&e, 1000).metrics[0].used, Some(5.0));
}

#[test]
fn an_online_success_does_not_hide_a_local_failure() {
    let fake = Fake::new(&FAKE);
    fake.local.lock().unwrap().push(Err(err(ErrorKind::PermissionDenied)));
    fake.online.lock().unwrap().push(Ok(quota("api", 50.0, None, Source::ProviderApi, 100)));
    let mut e = engine(fake);
    local(&mut e, 100);
    poll(&mut e, 100);
    let p = only(&e, 100);
    assert_eq!((p.metrics.len(), p.error.map(|e| e.kind)), (1, Some(ErrorKind::PermissionDenied)));
}

#[test]
fn a_collector_that_hangs_every_time_cannot_pile_up_jobs() {
    let fake = Fake::new(&FAKE);
    let mut e = engine(fake);
    let mut issued = Vec::new();
    for _ in 0..20 {
        // Each job is taken and never reported back, then its lease is left to run out.
        issued.extend(e.due_online(0));
        e.age_last_poll("fake", Duration::from_secs(100_000));
    }
    assert_eq!(issued.len(), 3, "at most three outstanding jobs per provider");
    let p = only(&e, 0);
    assert!(!p.refreshing);
    assert_eq!(p.error.map(|e| e.kind), Some(ErrorKind::Unavailable), "a check that never finished is reported");
    // Reconfiguring does not reset the count: those jobs are still running.
    let mut s = Settings::default();
    s.providers.insert("fake".into(), ProviderCfg { enabled: true, dir: std::env::temp_dir().join("quotis-hang").display().to_string() });
    e.set_settings(s);
    e.set_settings(Settings::default());
    assert!(e.due_online(0).is_empty(), "still three outstanding");
    // When one of them finally reports, even for an old configuration, a slot frees up.
    e.finish(&issued[0], Some(Err(err(ErrorKind::Offline))), 0);
    e.age_last_poll("fake", Duration::from_secs(100_000));
    assert_eq!(e.due_online(0).len(), 1);
}

#[test]
fn an_older_local_job_finishing_late_never_overwrites_a_newer_one() {
    let fake = Fake::new(&FAKE);
    let mut e = engine(fake);
    let older = e.local_jobs().remove(0);
    let newer = e.local_jobs().remove(0);
    // Same second, so timestamps cannot order them: only the order of issue can.
    e.finish(&newer, Some(Ok(quota("q", 80.0, None, Source::LocalLog, 1000))), 1000);
    e.finish(&older, Some(Ok(quota("q", 20.0, None, Source::LocalLog, 1000))), 1000);
    assert_eq!(only(&e, 1000).metrics[0].used, Some(80.0));
}

#[test]
fn a_lost_online_job_does_not_block_polling_forever_and_its_late_answer_is_ignored() {
    let fake = Fake::new(&FAKE);
    fake.online.lock().unwrap().push(Ok(quota("q", 40.0, None, Source::ProviderApi, 5000)));
    let mut e = engine(fake);
    let lost = e.due_online(0).remove(0);
    assert!(only(&e, 0).refreshing);
    e.age_last_poll("fake", Duration::from_secs(100));
    assert!(e.due_online(100).is_empty() && e.refresh_online().is_empty(), "still inside the lease: one poll in flight");
    // The job never reports back (hung subprocess, dead thread). After the lease it is written off.
    e.age_last_poll("fake", Duration::from_secs(100_000));
    assert!(!only(&e, 0).refreshing, "the UI stops showing a refresh that is not happening");
    assert_eq!(poll(&mut e, 5000), 1, "polling resumes");
    assert!(only(&e, 5000).error.is_none(), "and a good answer clears the timeout");
    assert_eq!(only(&e, 5000).metrics[0].used, Some(40.0));
    // The lost job finally answers. It must not overwrite the newer result.
    e.finish(&lost, Some(Ok(quota("q", 1.0, None, Source::ProviderApi, 6000))), 6000);
    assert_eq!(only(&e, 6000).metrics[0].used, Some(40.0));
}

#[test]
fn a_snapshot_read_later_is_rejudged_for_the_time_it_is_read() {
    let fake = Fake::new(&FAKE);
    fake.online.lock().unwrap().push(Ok(quota("q", 90.0, Some(2000), Source::ProviderApi, 1500)));
    let mut e = engine(fake);
    poll(&mut e, 1500);
    let mut snap = e.snapshot(1600);
    assert_eq!((snap.providers[0].metrics[0].freshness, snap.stale_after_secs), (Freshness::Fresh, 900));
    // The same snapshot, read from disk after the window reset.
    let mut later = snap.clone();
    later.refresh(2001);
    assert_eq!(later.providers[0].metrics[0].freshness, Freshness::Expired);
    // And one read long after, with no reset involved.
    snap.providers[0].metrics[0].resets_at = None;
    snap.refresh(1500 + 901);
    assert_eq!(snap.providers[0].metrics[0].freshness, Freshness::Stale);
    // A snapshot written before the budget field existed still parses, with the default budget.
    let old = r#"{"schema_version":1,"generated_at":1,"app_version":"2.0.0","providers":[]}"#;
    assert_eq!(cache::parse(old).unwrap().stale_after_secs, 900);
}

#[test]
fn duplicate_old_reports_do_not_release_other_workers() {
    let mut e = engine(Fake::new(&FAKE));
    let mut old = Vec::new();
    for _ in 0..3 {
        old.extend(e.due_online(0));
        e.age_last_poll("fake", Duration::from_secs(100_000));
    }
    let mut s = Settings::default();
    s.providers.insert("fake".into(), ProviderCfg { enabled: true, dir: std::env::temp_dir().join("quotis-reconfigured").display().to_string() });
    e.set_settings(s);
    e.finish(&old[0], None, 0);
    assert_eq!(e.due_online(0).len(), 1);
    for _ in 0..10 {
        e.finish(&old[0], None, 0);
        e.age_last_poll("fake", Duration::from_secs(100_000));
        assert!(e.due_online(0).is_empty(), "a duplicate completion cannot free another job's slot");
    }
}

#[test]
fn local_failure_survives_online_failure_then_success() {
    let fake = Fake::new(&FAKE);
    fake.local.lock().unwrap().push(Err(err(ErrorKind::PermissionDenied)));
    fake.online.lock().unwrap().extend([Err(err(ErrorKind::Offline)), Ok(quota("api", 20.0, None, Source::ProviderApi, 100))]);
    let mut e = engine(fake);
    local(&mut e, 100);
    poll(&mut e, 100);
    e.age_last_poll("fake", Duration::from_secs(100_000));
    poll(&mut e, 100);
    assert_eq!(only(&e, 100).error.map(|e| e.kind), Some(ErrorKind::PermissionDenied));
}

#[test]
fn local_workers_stay_bounded_through_reconfiguration() {
    let mut e = engine(Fake::new(&FAKE));
    let mut issued = Vec::new();
    for i in 0..20 {
        let mut s = Settings::default();
        s.providers.insert("fake".into(), ProviderCfg { enabled: true, dir: std::env::temp_dir().join(format!("quotis-local-{i}")).display().to_string() });
        e.set_settings(s);
        issued.extend(e.local_jobs());
    }
    assert_eq!(issued.len(), 3, "reconfiguration must not allocate unlimited local workers");
    e.finish(&issued[0], None, 0);
    assert_eq!(e.local_jobs().len(), 1);
    e.finish(&issued[0], None, 0);
    assert!(e.local_jobs().is_empty(), "a duplicate local report frees no slot");
}

struct BlockingLocal;
impl Provider for BlockingLocal {
    fn info(&self) -> &'static Info {
        &FAKE
    }
    fn default_dir(&self) -> PathBuf {
        PathBuf::from("/fake")
    }
    fn detect(&self, _: &Path) -> bool {
        true
    }
    fn collect_local(&self, _: &Path, _: i64) -> Outcome {
        std::thread::sleep(Duration::from_millis(300));
        None
    }
}

#[test]
fn one_shot_local_collection_has_an_overall_deadline() {
    let mut e = Engine::with_providers(Settings::default(), vec![Arc::new(BlockingLocal)]);
    let start = std::time::Instant::now();
    let snap = e.collect_once_with_timeout(false, Duration::from_millis(30));
    assert!(start.elapsed() < Duration::from_millis(200), "local collector delayed the entire live command");
    assert_eq!(snap.providers[0].error.as_ref().map(|e| e.kind), Some(ErrorKind::Unavailable));
    assert!(snap.providers[0].metrics.is_empty());
}

#[test]
fn retry_after_is_not_shortened_even_beyond_a_week() {
    let fake = Fake::new(&FAKE);
    fake.online.lock().unwrap().push(Err(ProviderError { retry_after_secs: Some(u64::MAX), ..err(ErrorKind::RateLimited) }));
    let mut e = engine(fake);
    poll(&mut e, 0);
    assert_eq!(e.backoff_secs("fake"), u64::MAX);
    e.age_last_poll("fake", Duration::from_secs(100_000));
    assert!(e.due_online(0).is_empty());
    assert!(e.refresh_online().is_empty());
    assert!(e.all_online().is_empty(), "one-shot requests also respect the server delay");
}

#[test]
fn retry_after_starts_when_the_response_arrives() {
    let mut e = engine(Fake::new(&FAKE));
    let job = e.all_online().remove(0);
    e.age_last_poll("fake", Duration::from_secs(4000));
    e.finish(&job, Some(Err(ProviderError { retry_after_secs: Some(120), ..err(ErrorKind::RateLimited) })), 0);
    assert!(e.all_online().is_empty(), "slow request time cannot consume the server's retry delay");
    assert!(e.refresh_online().is_empty());
    assert!(e.due_online(0).is_empty());
    e.age_last_poll("fake", Duration::from_secs(4000));
    assert_eq!(e.all_online().len(), 1);
}

#[test]
fn capped_old_workers_report_unavailable_after_reconfiguration() {
    let mut e = engine(Fake::new(&FAKE));
    for i in 0..4 {
        let mut s = Settings::default();
        s.providers.insert("fake".into(), ProviderCfg { enabled: true, dir: std::env::temp_dir().join(format!("quotis-cap-error-{i}")).display().to_string() });
        e.set_settings(s);
        e.local_jobs();
        e.all_online();
    }
    let p = e.collect_once_with_timeout(true, Duration::from_millis(5)).providers.remove(0);
    assert_eq!(p.error.as_ref().map(|e| e.kind), Some(ErrorKind::Unavailable), "exhausted workers are not an undetected source");
    assert_eq!(p.status, Status::Error);
}
