//! The monitoring engine: owns per-provider state, decides what to collect and when, and
//! builds normalized snapshots. It does no I/O itself and spawns no threads: callers take
//! `Job`s, run them outside any lock, and hand the results back to `finish`.

use crate::model::*;
use crate::providers::{registry, Info, Provider};
use crate::settings::{resolve_dir, Settings};
use std::collections::HashMap;
use std::hash::BuildHasher;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

const MAX_BACKOFF: u64 = 3600;
/// How soon after local activity or a passed reset an online provider may be re-polled.
const EAGER_SECS: u64 = 120;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Local,
    Online,
}

/// One unit of collection work. Running it touches the filesystem or the network;
/// it holds no reference to the engine, so it runs without the engine lock.
#[derive(Clone)]
pub struct Job {
    pub provider: Arc<dyn Provider>,
    pub dir: PathBuf,
    pub kind: Kind,
    /// The provider configuration this job was created for. A result from an older
    /// configuration is dropped, even if the folder was since changed back.
    generation: u64,
}

pub type Outcome = Option<Result<Collected, ProviderError>>;

impl Job {
    pub fn id(&self) -> &'static str {
        self.provider.info().id
    }

    /// Runs the collection. A panicking adapter becomes an error for that provider only.
    pub fn run(&self, now: i64) -> Outcome {
        let work = || match self.kind {
            Kind::Local => self.provider.collect_local(&self.dir, now),
            Kind::Online => self.provider.collect_online(&self.dir, now),
        };
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(work))
            .unwrap_or_else(|_| Some(Err(ProviderError::new(ErrorKind::Internal, "internal error in this provider"))))
    }
}

#[derive(Default)]
struct State {
    dir: PathBuf,
    enabled: bool,
    detected: bool,
    /// Bumped every time this provider is reconfigured.
    generation: u64,
    metrics: Vec<Metric>,
    notes: Vec<String>,
    account: Option<String>,
    error: Option<ProviderError>,
    observed_at: Option<i64>,
    refreshing: bool,
    last_poll: Option<Instant>,
    backoff: u64,
    fails: u32,
    /// Local activity was seen since the last online poll.
    dirty: bool,
    /// Per-process random spread (0-10%) added to this provider's poll interval.
    jitter_pct: u64,
}

/// A provider's static facts plus where Quotis is looking for it.
#[derive(Clone, Debug, serde::Serialize)]
pub struct ProviderStatus {
    #[serde(flatten)]
    pub info: Info,
    pub enabled: bool,
    pub found: bool,
    pub path: String,
}

pub struct Engine {
    settings: Settings,
    providers: Vec<Arc<dyn Provider>>,
    states: HashMap<&'static str, State>,
    app_version: String,
}

impl Engine {
    pub fn new(settings: Settings) -> Self {
        Self::with_providers(settings, registry())
    }

    pub fn with_providers(settings: Settings, providers: Vec<Arc<dyn Provider>>) -> Self {
        let seed = std::collections::hash_map::RandomState::new();
        let states = providers.iter().map(|p| (p.info().id, State { jitter_pct: seed.hash_one(p.info().id) % 11, ..Default::default() })).collect();
        let mut e = Engine { settings: Settings::default(), providers, states, app_version: env!("CARGO_PKG_VERSION").into() };
        e.set_settings(settings);
        e
    }

    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    /// Records where the overlay was dragged to. Cheap: touches no provider state.
    pub fn set_position(&mut self, position: Option<(i32, i32)>) {
        self.settings.position = position;
    }

    /// Applies new settings. A provider whose folder or on/off state changed starts clean.
    pub fn set_settings(&mut self, mut settings: Settings) {
        settings.sanitize();
        for p in &self.providers {
            let info = p.info();
            let cfg = settings.providers.get(info.id);
            let enabled = cfg.map_or(info.default_enabled, |c| c.enabled);
            let dir = resolve_dir(cfg.map_or("", |c| &c.dir), p.default_dir());
            let st = self.states.get_mut(info.id).expect("state per provider");
            if st.dir != dir || st.enabled != enabled {
                *st = State { dir, enabled, jitter_pct: st.jitter_pct, generation: st.generation + 1, ..Default::default() };
            }
            st.detected = enabled && p.detect(&st.dir);
        }
        self.settings = settings;
    }

    fn base_secs(&self) -> u64 {
        self.settings.refresh_min.clamp(1, 60) * 60
    }

    fn job(&self, p: &Arc<dyn Provider>, kind: Kind) -> Job {
        let st = &self.states[p.info().id];
        Job { provider: p.clone(), dir: st.dir.clone(), kind, generation: st.generation }
    }

    fn enabled(&self) -> impl Iterator<Item = &Arc<dyn Provider>> {
        self.providers.iter().filter(|p| self.states[p.info().id].enabled)
    }

    /// Local collection for every enabled provider.
    pub fn local_jobs(&self) -> Vec<Job> {
        self.enabled().map(|p| self.job(p, Kind::Local)).collect()
    }

    /// Local collection for the providers whose watched folder contains a changed path.
    /// Those providers are also flagged for an early online re-poll.
    pub fn local_jobs_for(&mut self, changed: &[PathBuf]) -> Vec<Job> {
        let hit: Vec<Arc<dyn Provider>> =
            self.enabled().filter(|p| p.watch_dir(&self.states[p.info().id].dir).is_some_and(|w| changed.iter().any(|c| c.starts_with(&w)))).cloned().collect();
        for p in &hit {
            self.states.get_mut(p.info().id).expect("state").dirty = true;
        }
        hit.iter().map(|p| self.job(p, Kind::Local)).collect()
    }

    /// Folders worth watching for changes.
    pub fn watch_dirs(&self) -> Vec<PathBuf> {
        self.enabled().filter_map(|p| p.watch_dir(&self.states[p.info().id].dir)).filter(|d| d.is_dir()).collect()
    }

    /// Online polls that are due now. Each returned job is marked as refreshing; the caller
    /// must pass its outcome to `finish`.
    pub fn due_online(&mut self, now: i64) -> Vec<Job> {
        let base = self.base_secs();
        let due: Vec<Arc<dyn Provider>> = self
            .enabled()
            .filter(|p| p.has_online())
            .filter(|p| {
                let st = &self.states[p.info().id];
                if st.refreshing {
                    return false;
                }
                let Some(last) = st.last_poll else { return true };
                let secs = last.elapsed().as_secs();
                let wait = base.max(st.backoff) * (100 + st.jitter_pct) / 100;
                let reset_passed = st.metrics.iter().any(|m| m.source == Source::ProviderApi && m.resets_at.is_some_and(|r| r <= now && m.observed_at < r));
                secs >= wait || ((st.dirty || reset_passed) && secs >= EAGER_SECS.max(st.backoff))
            })
            .cloned()
            .collect();
        self.begin(due)
    }

    /// Online polls the user asked for by hand. Still honours backoff and a 30 second floor,
    /// so a refresh button cannot hammer a provider that asked for patience.
    pub fn refresh_online(&mut self) -> Vec<Job> {
        let ready = self
            .enabled()
            .filter(|p| p.has_online())
            .filter(|p| {
                let st = &self.states[p.info().id];
                !st.refreshing && st.last_poll.is_none_or(|t| t.elapsed().as_secs() >= st.backoff.max(30))
            })
            .cloned()
            .collect();
        self.begin(ready)
    }

    /// Online polls for every enabled provider, ignoring the schedule (startup, `--live`).
    pub fn all_online(&mut self) -> Vec<Job> {
        let all = self.enabled().filter(|p| p.has_online()).cloned().collect();
        self.begin(all)
    }

    fn begin(&mut self, providers: Vec<Arc<dyn Provider>>) -> Vec<Job> {
        providers
            .iter()
            .map(|p| {
                let st = self.states.get_mut(p.info().id).expect("state");
                st.refreshing = true;
                st.last_poll = Some(Instant::now());
                st.dirty = false;
                self.job(p, Kind::Online)
            })
            .collect()
    }

    /// Records the outcome of a job. Results for a provider that was since disabled or
    /// pointed at another folder are dropped.
    pub fn finish(&mut self, job: &Job, outcome: Outcome, now: i64) {
        let base = self.base_secs();
        let Some(st) = self.states.get_mut(job.id()) else { return };
        if !st.enabled || st.generation != job.generation {
            return;
        }
        if job.kind == Kind::Online {
            st.refreshing = false;
        }
        let has_local = |st: &State| st.metrics.iter().any(|m| m.source != Source::ProviderApi);
        match (job.kind, outcome) {
            (_, None) => {}
            (Kind::Local, Some(Ok(c))) => {
                st.detected = true;
                if st.error.as_ref().is_some_and(|e| e.kind == ErrorKind::NotSignedIn || st.metrics.is_empty()) {
                    st.error = None;
                }
                if st.metrics.is_empty() {
                    st.notes = c.notes;
                }
                merge(&mut st.metrics, c.metrics);
                st.observed_at = st.metrics.iter().map(|m| m.observed_at).max().or(st.observed_at);
            }
            (Kind::Local, Some(Err(e))) => {
                if st.metrics.is_empty() {
                    st.error = Some(e);
                }
            }
            (Kind::Online, Some(Ok(c))) => {
                st.detected = true;
                st.metrics.retain(|m| m.source != Source::ProviderApi);
                merge(&mut st.metrics, c.metrics);
                st.notes = c.notes;
                st.account = c.account;
                st.error = None;
                st.observed_at = Some(now);
                st.backoff = 0;
                st.fails = 0;
            }
            (Kind::Online, Some(Err(e))) => {
                st.fails = st.fails.saturating_add(1);
                st.backoff = match e.kind {
                    ErrorKind::RateLimited => e.retry_after_secs.unwrap_or((st.backoff * 2).max(300)),
                    // The first failure retries on the normal schedule; repeats back off.
                    _ => base.saturating_mul(1 << (st.fails - 1).min(6)),
                }
                .min(MAX_BACKOFF);
                match e.kind {
                    // No login: whatever the API said before no longer describes this account.
                    ErrorKind::NotSignedIn | ErrorKind::AuthExpired => {
                        st.metrics.retain(|m| m.source != Source::ProviderApi);
                        st.notes.clear();
                        // A tool that works from local files alone is not in error for lacking an online login.
                        st.error = (e.kind != ErrorKind::NotSignedIn || !has_local(st)).then_some(e);
                    }
                    // Transient: keep the last numbers (they age into "stale") and say why.
                    _ => st.error = Some(e),
                }
            }
        }
    }

    pub fn snapshot(&self, now: i64) -> Snapshot {
        let stale_after = 3 * self.base_secs() as i64;
        let providers = self
            .providers
            .iter()
            .map(|p| {
                let info = p.info();
                let st = &self.states[info.id];
                let mut metrics = st.metrics.clone();
                for m in &mut metrics {
                    m.refresh_freshness(now, stale_after);
                }
                let has_data = !metrics.is_empty() || !st.notes.is_empty();
                let status = match &st.error {
                    _ if !st.enabled => Status::Disabled,
                    _ if has_data => Status::Connected,
                    Some(e) if e.kind == ErrorKind::NotSignedIn => Status::NotDetected,
                    Some(_) => Status::Error,
                    None if st.detected => Status::Connected,
                    None => Status::NotDetected,
                };
                ProviderSnapshot {
                    id: info.id.into(),
                    name: info.name.into(),
                    vendor: info.vendor.into(),
                    account: st.account.clone(),
                    status,
                    error: st.error.clone().filter(|_| st.enabled),
                    observed_at: st.observed_at,
                    refreshing: st.refreshing,
                    metrics,
                    notes: st.notes.clone(),
                }
            })
            .collect();
        Snapshot { schema_version: SCHEMA_VERSION, generated_at: now, app_version: self.app_version.clone(), providers }
    }

    /// What Settings and `quotis-cli providers` show: every known provider and where it was looked for.
    pub fn statuses(&self) -> Vec<ProviderStatus> {
        self.providers
            .iter()
            .map(|p| {
                let st = &self.states[p.info().id];
                let path = if p.info().key_env.is_some() { String::new() } else { st.dir.display().to_string() };
                ProviderStatus { info: *p.info(), enabled: st.enabled, found: st.enabled && p.detect(&st.dir), path }
            })
            .collect()
    }

    /// Collects everything once and returns the snapshot: local sources inline, online
    /// sources in parallel. Bounded by the network timeout. Used by `quotis-cli --live`.
    pub fn collect_once(&mut self, online: bool) -> Snapshot {
        let now = crate::time::now();
        for job in self.local_jobs() {
            let out = job.run(now);
            self.finish(&job, out, now);
        }
        if online {
            let jobs = self.all_online();
            let outcomes: Vec<Outcome> = std::thread::scope(|s| {
                let handles: Vec<_> = jobs.iter().map(|j| s.spawn(move || j.run(now))).collect();
                handles.into_iter().map(|h| h.join().unwrap_or(None)).collect()
            });
            for (job, out) in jobs.iter().zip(outcomes) {
                self.finish(job, out, now);
            }
        }
        self.snapshot(crate::time::now())
    }

    /// Test and diagnostics hook: pretend the last poll of `id` happened `ago` in the past.
    pub fn age_last_poll(&mut self, id: &str, ago: Duration) {
        if let Some(st) = self.states.get_mut(id) {
            st.last_poll = Instant::now().checked_sub(ago);
        }
    }

    pub fn backoff_secs(&self, id: &str) -> u64 {
        self.states.get(id).map_or(0, |s| s.backoff)
    }

    pub fn dir_of(&self, id: &str) -> Option<&Path> {
        self.states.get(id).map(|s| s.dir.as_path())
    }
}

/// Merges new metrics into existing ones by id; for the same id the newer observation wins.
fn merge(existing: &mut Vec<Metric>, new: Vec<Metric>) {
    for m in new {
        match existing.iter_mut().find(|e| e.id == m.id) {
            Some(e) if m.observed_at >= e.observed_at => *e = m,
            Some(_) => {}
            None => existing.push(m),
        }
    }
}
