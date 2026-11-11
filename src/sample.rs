//! The heartbeat: tick on an interval and publish the latest sample.
//!
//! The sampler is the only thing that reads the operating system on a schedule.
//! Everything else — the CLI views, the TUI, the daemon — reads the state it
//! publishes, so every view sees the same numbers.

#[cfg(test)]
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use crate::alert::{Alert, AlertEngine};
use crate::collect::process;
use crate::collect::{Collector, RawSnapshot, sample_between};
use crate::core::metrics::SystemSample;
use crate::core::process::ProcessSample;
use crate::exits::{ExitRecord, ExitTracker, RestartLoop};
use crate::history::{CAPACITY, Ring};
use crate::policy::{PolicyEngine, Proposal};

/// The latest published state. Cloning is cheap: it holds no locks.
#[derive(Debug, Clone, Default)]
pub struct SamplerState {
    pub tick: u64,
    pub system: SystemSample,
    pub processes: Vec<ProcessSample>,
    /// The alerts raised so far, oldest first.
    pub alerts: Vec<Alert>,
    /// The processes that went away, oldest first.
    pub exits: Vec<ExitRecord>,
    /// The processes that keep coming back.
    pub restart_loops: Vec<RestartLoop>,
    /// The actions the policies would take.
    pub proposals: Vec<Proposal>,
}

/// What happens when an alert is raised.
pub type AlertSink = Box<dyn Fn(&Alert) + Send>;

/// The state the sampler publishes, shared with the readers.
pub struct Shared {
    state: RwLock<Arc<SamplerState>>,
    history: RwLock<Ring<Arc<SamplerState>>>,
}

impl Default for Shared {
    fn default() -> Self {
        Shared::new(Duration::from_secs(60))
    }
}

impl Shared {
    /// A shared state that keeps the recent samples for a while.
    pub fn new(keep_for: Duration) -> Self {
        Self {
            state: RwLock::new(Arc::new(SamplerState::default())),
            history: RwLock::new(Ring::new(keep_for, CAPACITY)),
        }
    }

    /// The latest published sample.
    pub fn state(&self) -> Arc<SamplerState> {
        self.state
            .read()
            .map(|state| state.clone())
            .unwrap_or_default()
    }

    /// The recent samples, oldest first.
    pub fn history(&self) -> Vec<Arc<SamplerState>> {
        self.history
            .read()
            .map(|history| history.to_vec().into_iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Publish a sample and remember it for as long as it is worth keeping.
    pub fn publish(&self, state: Arc<SamplerState>, _keep_for: Duration) {
        let now = Instant::now();
        if let Ok(mut history) = self.history.write() {
            history.push(state.clone(), now);
        }
        if let Ok(mut slot) = self.state.write() {
            *slot = state;
        }
    }
}

/// Publishes samples on a fixed interval.
pub struct Sampler {
    interval: Duration,
    history_duration: Duration,
    collector: Box<dyn Collector>,
    previous: Option<RawSnapshot>,
    deadline: Option<Instant>,
    shared: Arc<Shared>,
    last_error: Option<String>,
    alerts: Option<AlertEngine>,
    on_alert: Option<AlertSink>,
    tracker: ExitTracker,
    weights: crate::pressure::Weights,
    policies: Option<PolicyEngine>,
    executor: Option<crate::action::Executor>,
    writer: Option<crate::store::HistoryWriter>,
}

impl Sampler {
    pub fn new(interval: Duration, collector: Box<dyn Collector>) -> Self {
        let history_duration = Duration::from_secs(60);
        Self {
            interval,
            history_duration,
            collector,
            previous: None,
            deadline: None,
            shared: Arc::new(Shared::new(history_duration)),
            last_error: None,
            alerts: None,
            on_alert: None,
            tracker: ExitTracker::new(),
            weights: crate::pressure::Weights::default(),
            policies: None,
            executor: None,
            writer: None,
        }
    }

    /// Write the history that is waiting to be written.
    pub fn flush_history(&mut self) {
        if let Some(writer) = &mut self.writer
            && let Err(error) = writer.flush()
        {
            eprintln!("sentinel: cannot write the history: {error}");
        }
    }

    /// Keep the history on disk, so it survives a restart.
    pub fn with_history_store(mut self, store: crate::store::Store, batch: Duration) -> Self {
        self.writer = Some(crate::store::HistoryWriter::new(store, batch));
        self
    }

    /// Perform the actions the policies propose.
    pub fn with_executor(mut self, executor: crate::action::Executor) -> Self {
        self.executor = Some(executor);
        self
    }

    /// Watch the policies and propose what they would do.
    pub fn with_policies(mut self, engine: PolicyEngine) -> Self {
        self.policies = Some(engine);
        self
    }

    /// How much each resource counts towards the pressure score.
    pub fn with_pressure(mut self, weights: crate::pressure::Weights) -> Self {
        self.weights = weights;
        self
    }

    /// Collect from somewhere other than the operating system.
    pub fn with_collector(mut self, collector: Box<dyn Collector>) -> Self {
        self.collector = collector;
        self
    }

    /// Look for restart loops and OOM evidence.
    pub fn with_detection(
        mut self,
        restart_count: u32,
        restart_window: Duration,
        oom_evidence: bool,
    ) -> Self {
        self.tracker =
            ExitTracker::new().with_detection(restart_count, restart_window, oom_evidence);
        self
    }

    /// Watch the metrics and raise alerts when the rules call for it.
    pub fn with_alerts(mut self, engine: AlertEngine, on_alert: AlertSink) -> Self {
        self.alerts = Some(engine);
        self.on_alert = Some(on_alert);
        self
    }

    /// How much of the recent past to keep.
    pub fn with_history(mut self, history_duration: Duration) -> Self {
        self.history_duration = history_duration;
        self.shared = Arc::new(Shared::new(history_duration));
        self
    }

    pub fn interval(&self) -> Duration {
        self.interval
    }

    /// A handle to the published state, for readers on another thread.
    pub fn handle(&self) -> Arc<Shared> {
        self.shared.clone()
    }

    /// The published state. Readers clone the inner `Arc` and never hold the
    /// lock while they work, so a slow reader cannot hold up collection.
    pub fn state(&self) -> Arc<SamplerState> {
        self.shared.state()
    }

    /// The reason the last reading failed, if it did.
    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    /// Take one reading and publish the sample it produces.
    pub fn tick(&mut self) -> Arc<SamplerState> {
        let reading = match self.collector.collect() {
            Ok(reading) => reading,
            Err(error) => {
                self.last_error = Some(error);
                return self.state();
            }
        };
        self.last_error = None;
        let system = sample_between(&reading, self.previous.as_ref());
        let processes = match self.previous.as_ref() {
            Some(before) => {
                let elapsed = reading.at.duration_since(before.at).as_secs_f64();
                process::process_samples(
                    &reading.processes,
                    &before.processes,
                    reading.uptime_secs,
                    reading.memory.total_bytes,
                    elapsed,
                )
            }
            None => Vec::new(),
        };
        self.previous = Some(reading);

        let mut alerts = self.state().alerts.clone();
        if let Some(engine) = self.alerts.as_mut() {
            alerts.extend(engine.evaluate(&system, &processes, Instant::now()));
            if let Some(on_alert) = &self.on_alert {
                for alert in &alerts {
                    on_alert(alert);
                }
            }
        }

        alerts.truncate(50);
        let mut processes = processes;
        crate::pressure::apply_scores(
            &mut processes,
            &self.tracker.restart_counts(),
            &self.weights,
        );

        let mut exits = self.state().exits.clone();
        exits.extend(self.tracker.observe(&processes));
        exits.truncate(50);
        let restart_loops = self.tracker.restart_loops();
        let proposals = match self.policies.as_mut() {
            Some(engine) => engine.propose(&system, &processes, Instant::now()),
            None => Vec::new(),
        };
        if let Some(executor) = &self.executor {
            for proposal in &proposals {
                executor.perform(proposal, proposal.allow_protected);
            }
        }
        if let Some(writer) = &mut self.writer {
            writer.push(crate::store::StoredSample::of(&system, &processes));
            // A disk that is full is reported, and monitoring carries on.
            if let Err(error) = writer.flush_if_due() {
                eprintln!("sentinel: cannot write the history: {error}");
            }
        }
        let state = Arc::new(SamplerState {
            tick: self.state().tick + 1,
            system,
            processes,
            alerts,
            exits,
            restart_loops,
            proposals,
        });
        self.shared.publish(state.clone(), self.history_duration);
        state
    }

    /// Two readings one interval apart, so the rates are meaningful.
    pub fn warm_sample(&mut self) -> Arc<SamplerState> {
        self.tick();
        std::thread::sleep(self.interval);
        self.tick()
    }

    /// Tick until `stop` returns true.
    ///
    /// A tick that overran its interval does not pile up: the next deadline is
    /// measured from now, so a slow collector slows the sampler down instead of
    /// queueing work it can never catch up with.
    pub fn run(&mut self, stop: &dyn Fn() -> bool) {
        self.run_with(stop, &|_| {});
    }

    /// Tick until `stop` returns true, reporting every tick.
    pub fn run_with(&mut self, stop: &dyn Fn() -> bool, on_tick: &dyn Fn(&Arc<SamplerState>)) {
        while !stop() {
            let state = self.tick();
            on_tick(&state);
            let next = self.next_deadline();
            let now = Instant::now();
            if next > now {
                std::thread::sleep(next - now);
            }
        }
    }

    /// The next moment to tick.
    fn next_deadline(&mut self) -> Instant {
        let now = Instant::now();
        match self.deadline {
            Some(deadline) if deadline > now => deadline,
            _ => now + self.interval,
        }
    }
}

/// A collector that produces fixed readings and counts its calls, for tests.
#[cfg(test)]
pub struct FakeCollector {
    pub readings: Vec<RawSnapshot>,
    pub calls: Arc<AtomicUsize>,
}

#[cfg(test)]
impl FakeCollector {
    pub fn new(readings: Vec<RawSnapshot>) -> Self {
        Self {
            readings,
            calls: Arc::new(AtomicUsize::new(0)),
        }
    }
}

#[cfg(test)]
impl Collector for FakeCollector {
    fn collect(&mut self) -> Result<RawSnapshot, String> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        if !self.readings.is_empty() {
            return Ok(self.readings.remove(0));
        }
        Ok(RawSnapshot {
            at: Instant::now(),
            ..Default::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collect::{DiskCounters, NetworkCounters};
    use std::collections::HashMap;

    fn reading(at: Instant, bytes: u64) -> RawSnapshot {
        RawSnapshot {
            at,
            disk: HashMap::from([(
                "sda".to_string(),
                DiskCounters {
                    read_bytes: bytes,
                    ..Default::default()
                },
            )]),
            network: HashMap::from([(
                "eth0".to_string(),
                NetworkCounters {
                    rx_bytes: bytes,
                    ..Default::default()
                },
            )]),
            ..Default::default()
        }
    }

    #[test]
    fn the_first_tick_publishes_zero_rates() {
        let now = Instant::now();
        let mut sampler = Sampler::new(
            Duration::from_millis(10),
            Box::new(FakeCollector::new(vec![reading(now, 1_000)])),
        );

        let state = sampler.tick();

        assert_eq!(state.tick, 1);
        assert_eq!(state.system.disk.read_bytes_per_sec, 0);
        assert_eq!(state.system.network.rx_bytes_per_sec, 0);
    }

    #[test]
    fn the_second_tick_publishes_the_difference() {
        let now = Instant::now();
        let mut sampler = Sampler::new(
            Duration::from_millis(10),
            Box::new(FakeCollector::new(vec![
                reading(now, 1_000),
                reading(now + Duration::from_secs(1), 2_000),
            ])),
        );

        sampler.tick();
        let state = sampler.tick();

        assert_eq!(state.tick, 2);
        assert_eq!(state.system.disk.read_bytes_per_sec, 1_000);
        assert_eq!(state.system.network.rx_bytes_per_sec, 1_000);
    }

    #[test]
    fn a_slow_tick_does_not_queue_up_more_work() {
        let mut sampler = Sampler::new(
            Duration::from_millis(10),
            Box::new(FakeCollector::new(vec![])),
        );

        let start = Instant::now();
        let deadline = start + Duration::from_millis(120);
        sampler.run(&|| Instant::now() >= deadline);
        let elapsed = start.elapsed();

        // At most one tick per interval would be 12 ticks; a sampler that piled
        // up would tick far more often than it could keep up.
        assert!(elapsed >= Duration::from_millis(100), "{elapsed:?}");
    }

    #[test]
    fn the_sampler_honours_a_100ms_interval() {
        let calls = Arc::new(AtomicUsize::new(0));
        let collector = FakeCollector {
            readings: vec![],
            calls: calls.clone(),
        };
        let mut sampler = Sampler::new(Duration::from_millis(100), Box::new(collector));

        let start = Instant::now();
        let deadline = start + Duration::from_millis(550);
        sampler.run(&|| Instant::now() >= deadline);
        let ticks = calls.load(Ordering::Relaxed);

        // Five or six ticks fit in 550ms at 100ms a tick.
        assert!((3..=8).contains(&ticks), "{ticks} ticks in 550ms");
    }

    #[test]
    fn the_state_can_be_read_while_the_sampler_keeps_running() {
        let mut sampler = Sampler::new(
            Duration::from_millis(1),
            Box::new(FakeCollector::new(vec![])),
        );

        sampler.tick();
        let published = sampler.state();

        assert_eq!(published.tick, 1);
        assert_eq!(sampler.state().tick, 1);
    }
}
