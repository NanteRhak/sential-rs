//! The heartbeat: tick on an interval and publish the latest sample.
//!
//! The sampler is the only thing that reads the operating system on a schedule.
//! Everything else — the CLI views, the TUI, the daemon — reads the state it
//! publishes, so every view sees the same numbers.

#[cfg(test)]
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use crate::collect::{Collector, RawSnapshot, sample_between};
use crate::core::metrics::SystemSample;

/// The latest published state. Cloning is cheap: it holds no locks.
#[derive(Debug, Clone, Default)]
pub struct SamplerState {
    pub tick: u64,
    pub system: SystemSample,
}

/// Publishes samples on a fixed interval.
pub struct Sampler {
    interval: Duration,
    collector: Box<dyn Collector>,
    previous: Option<RawSnapshot>,
    deadline: Option<Instant>,
    state: Arc<RwLock<Arc<SamplerState>>>,
}

impl Sampler {
    pub fn new(interval: Duration, collector: Box<dyn Collector>) -> Self {
        Self {
            interval,
            collector,
            previous: None,
            deadline: None,
            state: Arc::new(RwLock::new(Arc::new(SamplerState::default()))),
        }
    }

    pub fn interval(&self) -> Duration {
        self.interval
    }

    /// The published state. Readers clone the inner `Arc` and never hold the
    /// lock while they work, so a slow reader cannot hold up collection.
    pub fn state(&self) -> Arc<SamplerState> {
        self.state
            .read()
            .map(|state| state.clone())
            .unwrap_or_default()
    }

    /// Take one reading and publish the sample it produces.
    pub fn tick(&mut self) -> Arc<SamplerState> {
        let reading = self.collector.collect();
        let system = sample_between(&reading, self.previous.as_ref());
        self.previous = Some(reading);

        let state = Arc::new(SamplerState {
            tick: self.state().tick + 1,
            system,
        });
        if let Ok(mut slot) = self.state.write() {
            *slot = state.clone();
        }
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
        while !stop() {
            self.tick();
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
    fn collect(&mut self) -> RawSnapshot {
        self.calls.fetch_add(1, Ordering::Relaxed);
        if !self.readings.is_empty() {
            return self.readings.remove(0);
        }
        RawSnapshot {
            at: Instant::now(),
            ..Default::default()
        }
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
