//! A bounded ring of the most recent samples.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// The most samples kept, whatever the history duration says.
///
/// A long history sampled at 100ms would otherwise hold tens of thousands of
/// samples, and the point of the ring is that memory use stays flat.
pub const CAPACITY: usize = 10_000;

/// A ring of recent items, bounded by time and by count.
///
/// The oldest items are dropped when the ring is full, so the newest are
/// always the ones that are kept.
pub struct Ring<T> {
    items: VecDeque<(Instant, T)>,
    keep_for: Duration,
    capacity: usize,
}

impl<T> Ring<T> {
    pub fn new(keep_for: Duration, capacity: usize) -> Self {
        Self {
            items: VecDeque::new(),
            keep_for,
            capacity: capacity.max(1),
        }
    }

    /// Add an item, dropping the ones that are too old or too many.
    pub fn push(&mut self, item: T, now: Instant) {
        self.items.push_back((now, item));
        while self.items.len() > self.capacity {
            self.items.pop_front();
        }
        while self
            .items
            .front()
            .is_some_and(|(at, _)| now.duration_since(*at) > self.keep_for)
        {
            self.items.pop_front();
        }
    }

    /// The items, oldest first.
    pub fn items(&self) -> impl Iterator<Item = &T> {
        self.items.iter().map(|(_, item)| item)
    }

    /// The items, oldest first, as owned values.
    pub fn to_vec(&self) -> Vec<&T> {
        self.items.iter().map(|(_, item)| item).collect()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ring_keeps_the_newest_items_when_it_overflows() {
        let mut ring = Ring::new(Duration::from_secs(60), 3);
        let now = Instant::now();

        for index in 0..10 {
            ring.push(index, now);
        }

        assert_eq!(ring.len(), 3);
        assert_eq!(ring.to_vec(), [&7, &8, &9]);
    }

    #[test]
    fn the_ring_drops_the_items_that_are_too_old() {
        let mut ring = Ring::new(Duration::from_secs(1), 100);
        let start = Instant::now();

        ring.push(1, start);
        ring.push(2, start + Duration::from_secs(2));

        assert_eq!(ring.len(), 1);
        assert_eq!(ring.to_vec(), [&2]);
    }

    #[test]
    fn the_ring_keeps_everything_while_it_fits() {
        let mut ring = Ring::new(Duration::from_secs(60), 100);
        let now = Instant::now();

        ring.push(1, now);
        ring.push(2, now);

        assert_eq!(ring.len(), 2);
        assert!(!ring.is_empty());
    }
}
