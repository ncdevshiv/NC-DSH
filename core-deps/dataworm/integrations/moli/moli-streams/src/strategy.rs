//! Runtime-independent strategy and backpressure calculations.

/// A point-in-time view of a Stream controller's queuing strategy.
///
/// The snapshot contains no queue entries or JavaScript values. Runtime
/// adapters remain responsible for reading the current high water mark and
/// queue total from their traced storage before each decision.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StrategySnapshot {
    high_water_mark: f64,
    queue_total_size: f64,
}

impl StrategySnapshot {
    #[must_use]
    pub const fn new(high_water_mark: f64, queue_total_size: f64) -> Self {
        Self {
            high_water_mark,
            queue_total_size,
        }
    }

    #[must_use]
    pub const fn high_water_mark(self) -> f64 {
        self.high_water_mark
    }

    #[must_use]
    pub const fn queue_total_size(self) -> f64 {
        self.queue_total_size
    }

    #[must_use]
    pub fn desired_size(self) -> f64 {
        self.high_water_mark - self.queue_total_size
    }

    #[must_use]
    pub fn has_capacity(self) -> bool {
        self.desired_size() > 0.0
    }

    #[must_use]
    pub fn applies_backpressure(self) -> bool {
        self.desired_size() <= 0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desired_size_preserves_javascript_double_arithmetic() {
        let large = StrategySnapshot::new(0.0, 9_007_199_254_740_991.0 + 2.0);
        assert_eq!(
            large.desired_size(),
            0.0 - (9_007_199_254_740_991_f64 + 2.0)
        );

        let tiny = StrategySnapshot::new(0.0, 1.0 + 1e-16);
        assert_eq!(tiny.desired_size(), 0.0 - (1.0 + 1e-16));
    }

    #[test]
    fn capacity_and_backpressure_partition_zero() {
        let capacity = StrategySnapshot::new(1.0, 0.0);
        assert!(capacity.has_capacity());
        assert!(!capacity.applies_backpressure());

        let exact = StrategySnapshot::new(1.0, 1.0);
        assert!(!exact.has_capacity());
        assert!(exact.applies_backpressure());

        let over = StrategySnapshot::new(1.0, 2.0);
        assert!(!over.has_capacity());
        assert!(over.applies_backpressure());
    }

    #[test]
    fn infinite_high_water_mark_never_applies_finite_backpressure() {
        let snapshot = StrategySnapshot::new(f64::INFINITY, f64::MAX);
        assert_eq!(snapshot.desired_size(), f64::INFINITY);
        assert!(snapshot.has_capacity());
        assert!(!snapshot.applies_backpressure());
    }
}
