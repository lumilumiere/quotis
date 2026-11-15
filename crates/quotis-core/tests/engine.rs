//! Engine behaviour against scripted providers: scheduling, backoff, merging, staleness,
//! fault isolation and concurrent updates. No network, no real provider files.

use quotis_core::providers::{Info, Provider};
use quotis_core::*;
use std::path::{Path, PathBuf};
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
}

impl Fake {
    fn new(info: &'static Info) -> Arc<Self> {
        Arc::new(Fake { info, online: Default::default(), local: Default::default(), panic_online: false })
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
    Engine::with_providers(Settings::default(), vec![fake])
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
    assert_eq!(e.backoff_secs("fake"), 3600, "capped at one hour");
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
    let local = |e: &mut Engine| {
        let j = e.local_jobs().remove(0);
        let out = j.run(0);
        e.finish(&j, out, 0);
    };
    local(&mut e);
    assert_eq!(only(&e, 600).metrics[0].used, Some(10.0));
    poll(&mut e, 1000);
    assert_eq!(only(&e, 1000).metrics[0].used, Some(20.0), "newer online value replaces the older log value");
    local(&mut e);
    assert_eq!(only(&e, 1500).metrics[0].used, Some(30.0), "a newer log line replaces the online value");
    local(&mut e);
    assert_eq!(only(&e, 1500).metrics[0].used, Some(30.0), "an older log line never overwrites newer data");
    assert_eq!(only(&e, 1500).metrics.len(), 1);
}

#[test]
fn signed_out_online_is_quiet_when_local_data_exists() {
    let fake = Fake::new(&FAKE);
    fake.local.lock().unwrap().push(Ok(quota("q", 10.0, None, Source::LocalLog, 0)));
    fake.online.lock().unwrap().extend([Err(err(ErrorKind::NotSignedIn)), Err(err(ErrorKind::AuthExpired))]);
    let mut e = engine(fake);
    let j = e.local_jobs().remove(0);
    let out = j.run(0);
    e.finish(&j, out, 0);
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
    let bad = Arc::new(Fake { info: &FAKE, online: Default::default(), local: Default::default(), panic_online: true });
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
    let e = Engine::with_providers(s, vec![Fake::new(&FAKE)]);
    let st = e.statuses().remove(0);
    assert!(!st.enabled && !st.found, "a disabled provider's files and login are never touched");
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
    let job = e.lock().unwrap().local_jobs().remove(0);
    std::thread::scope(|s| {
        for t in 0..8i64 {
            let (e, job) = (e.clone(), job.clone());
            s.spawn(move || {
                for i in 0..200i64 {
                    let observed = t * 1000 + i;
                    let out = Some(Ok(quota("q", (observed % 100) as f64, None, Source::LocalLog, observed)));
                    e.lock().unwrap().finish(&job, out, observed);
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
