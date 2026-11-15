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
/// The longest a server's own Retry-After is honoured for.
const MAX_RETRY_AFTER: u64 = 86_400;
/// An online job that has not reported back after this long is presumed lost: the provider
/// may be polled again and a late answer from the lost job is ignored.
const LEASE_SECS: u64 = 180;
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
    /// Order in which jobs were handed out. An older local job never overwrites a newer one,
    /// and only the most recent online job for a provider is accepted.
    seq: u64,
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
    /// Which kind of collection the current error came from.
    error_src: Option<Kind>,
    /// Highest local job sequence committed so far.
    local_seq: u64,
    /// Sequence of the online job currently in flight.
    online_seq: u64,
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
    next_seq: u64,
}

/// A detection pass, prepared under the engine lock and run outside it: detection reads
/// credential files and databases and must never hold up the UI.
pub struct Probe(Vec<(Arc<dyn Provider>, PathBuf, u64)>);

impl Probe {
    pub fn run(self) -> Vec<(&'static str, u64, bool)> {
        let detect = |p: &Arc<dyn Provider>, dir: &Path| std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| p.detect(dir))).unwrap_or(false);
        self.0.into_iter().map(|(p, dir, generation)| (p.info().id, generation, detect(&p, &dir))).collect()
    }
}

impl Engine {
    pub fn new(settings: Settings) -> Self {
        Self::with_providers(settings, registry())
    }

    pub fn with_providers(settings: Settings, providers: Vec<Arc<dyn Provider>>) -> Self {
        let seed = std::collections::hash_map::RandomState::new();
        let states = providers.iter().map(|p| (p.info().id, State { jitter_pct: seed.hash_one(p.info().id) % 11, ..Default::default() })).collect();
        let mut e = Engine { settings: Settings::default(), providers, states, app_version: env!("CARGO_PKG_VERSION").into(), next_seq: 0 };
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

    /// Prepares a detection pass for every enabled provider. Disabled providers are never probed.
    pub fn probe(&self) -> Probe {
        Probe(self.enabled().map(|p| (p.clone(), self.states[p.info().id].dir.clone(), self.states[p.info().id].generation)).collect())
    }

    /// Records detection results, ignoring any for a provider that was reconfigured meanwhile.
    pub fn apply_probe(&mut self, results: Vec<(&'static str, u64, bool)>) {
        for (id, generation, found) in results {
            if let Some(st) = self.states.get_mut(id).filter(|st| st.generation == generation) {
                st.detected = found;
            }
        }
    }

    /// Detects inline. For one-shot callers (the CLI, tests) that hold no lock.
    pub fn probe_now(&mut self) {
        let results = self.probe().run();
        self.apply_probe(results);
    }

    /// Applies new settings. A provider whose folder or on/off state changed starts clean.
    /// Does no I/O: follow it with a probe to learn what is installed.
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
        }
        self.settings = settings;
    }

    fn base_secs(&self) -> u64 {
        self.settings.refresh_min.clamp(1, 60) * 60
    }

    fn job(&mut self, p: &Arc<dyn Provider>, kind: Kind) -> Job {
        self.next_seq += 1;
        let st = &self.states[p.info().id];
        Job { provider: p.clone(), dir: st.dir.clone(), kind, generation: st.generation, seq: self.next_seq }
    }

    fn in_flight(st: &State) -> bool {
        st.refreshing && st.last_poll.is_some_and(|t| t.elapsed().as_secs() < LEASE_SECS)
    }

    fn enabled(&self) -> impl Iterator<Item = &Arc<dyn Provider>> {
        self.providers.iter().filter(|p| self.states[p.info().id].enabled)
    }

    /// Local collection for every enabled provider.
    pub fn local_jobs(&mut self) -> Vec<Job> {
        let all: Vec<_> = self.enabled().cloned().collect();
        all.iter().map(|p| self.job(p, Kind::Local)).collect()
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

    /// Folders worth watching for changes. Pure: the caller checks which of them exist.
    pub fn watch_dirs(&self) -> Vec<PathBuf> {
        self.enabled().filter_map(|p| p.watch_dir(&self.states[p.info().id].dir)).collect()
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
                if Self::in_flight(st) {
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
                !Self::in_flight(st) && st.last_poll.is_none_or(|t| t.elapsed().as_secs() >= st.backoff.max(30))
            })
            .cloned()
            .collect();
        self.begin(ready)
    }

    /// Online polls for every enabled provider, ignoring the schedule (startup, `--live`).
    pub fn all_online(&mut self) -> Vec<Job> {
        let all = self.enabled().filter(|p| p.has_online() && !Self::in_flight(&self.states[p.info().id])).cloned().collect();
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
                let job = self.job(p, Kind::Online);
                self.states.get_mut(p.info().id).expect("state").online_seq = job.seq;
                job
            })
            .collect()
    }

    /// Records the outcome of a job. Dropped without effect: results for a provider that was
    /// since disabled or reconfigured, a local result older than one already recorded, and an
    /// online result from a job that is no longer the one in flight.
    pub fn finish(&mut self, job: &Job, mut outcome: Outcome, now: i64) {
        let base = self.base_secs();
        let Some(st) = self.states.get_mut(job.id()) else { return };
        if !st.enabled || st.generation != job.generation {
            return;
        }
        match job.kind {
            Kind::Local if job.seq < st.local_seq => return,
            Kind::Local => st.local_seq = job.seq,
            Kind::Online if job.seq != st.online_seq => return,
            Kind::Online => st.refreshing = false,
        }
        // A reading cannot be from the future. Without this, one bad timestamp in a log
        // would outrank every later real reading.
        if let Some(Ok(c)) = &mut outcome {
            for m in &mut c.metrics {
                m.observed_at = m.observed_at.min(now);
            }
        }
        let is_api = |m: &Metric| m.source == Source::ProviderApi;
        match (job.kind, outcome) {
            // The local source is gone (or this provider has none): its numbers go with it.
            (Kind::Local, None) => {
                st.metrics.retain(is_api);
                if st.error_src == Some(Kind::Local) {
                    st.error = None;
                }
            }
            (Kind::Online, None) => {}
            (Kind::Local, Some(Ok(c))) => {
                st.detected = true;
                if st.error_src == Some(Kind::Local) || st.error.as_ref().is_some_and(|e| e.kind == ErrorKind::NotSignedIn) {
                    st.error = None;
                }
                // A complete local pass replaces what the last local pass reported.
                st.metrics.retain(|m| is_api(m) || c.metrics.iter().any(|n| n.id == m.id));
                if !st.metrics.iter().any(is_api) {
                    st.notes = c.notes;
                }
                merge(&mut st.metrics, c.metrics);
                st.observed_at = st.metrics.iter().map(|m| m.observed_at).max().or(st.observed_at);
            }
            // Keep the last numbers (they age into "stale"), and say what went wrong.
            (Kind::Local, Some(Err(e))) => {
                st.error = Some(e);
                st.error_src = Some(Kind::Local);
            }
            (Kind::Online, Some(Ok(c))) => {
                st.detected = true;
                st.metrics.retain(|m| !is_api(m));
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
                st.backoff = match (e.kind, e.retry_after_secs) {
                    // The server said when to come back: believe it, up to a day.
                    (ErrorKind::RateLimited, Some(secs)) => secs.min(MAX_RETRY_AFTER),
                    (ErrorKind::RateLimited, None) => (st.backoff * 2).clamp(300, MAX_BACKOFF),
                    // The first failure retries on the normal schedule; repeats back off.
                    _ => base.saturating_mul(1 << (st.fails - 1).min(6)).min(MAX_BACKOFF),
                };
                st.error_src = Some(Kind::Online);
                match e.kind {
                    // No login: whatever the API said before no longer describes this account.
                    ErrorKind::NotSignedIn | ErrorKind::AuthExpired => {
                        st.metrics.retain(|m| !is_api(m));
                        st.notes.clear();
                        // A tool that works from local files alone is not in error for lacking an online login.
                        let has_local = !st.metrics.is_empty();
                        st.error = (e.kind != ErrorKind::NotSignedIn || !has_local).then_some(e);
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
                    refreshing: Self::in_flight(st),
                    metrics,
                    notes: st.notes.clone(),
                }
            })
            .collect();
        Snapshot { schema_version: SCHEMA_VERSION, generated_at: now, app_version: self.app_version.clone(), stale_after_secs: stale_after, providers }
    }

    /// What Settings and `quotis-cli providers` show: every known provider and where it was looked for.
    pub fn statuses(&self) -> Vec<ProviderStatus> {
        self.providers
            .iter()
            .map(|p| {
                let st = &self.states[p.info().id];
                let path = if p.info().key_env.is_some() { String::new() } else { st.dir.display().to_string() };
                ProviderStatus { info: *p.info(), enabled: st.enabled, found: st.enabled && st.detected, path }
            })
            .collect()
    }

    /// Collects everything once and returns the snapshot: local sources inline, online
    /// sources in parallel. Each request has a 10 second timeout and an adapter makes at most three. Used by `quotis-cli --live`.
    pub fn collect_once(&mut self, online: bool) -> Snapshot {
        let now = crate::time::now();
        self.probe_now();
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
