//! Blocking-worker budget for owner-local Tokio runtimes.
//!
//! Page and Worker JavaScript run on current-thread owner runtimes, but Web
//! Crypto and similar CPU/blocking operations use Tokio's separate blocking
//! pool. Tokio otherwise permits a much larger pool than Moli's crawler
//! process budget, so each web-exposed owner runtime applies this explicit cap.

const MAX_TOKIO_BLOCKING_THREADS: usize = 4;

pub(crate) fn tokio_blocking_thread_budget() -> usize {
    std::thread::available_parallelism()
        .map(tokio_blocking_thread_budget_for_parallelism)
        .unwrap_or(1)
}

fn tokio_blocking_thread_budget_for_parallelism(parallelism: std::num::NonZeroUsize) -> usize {
    parallelism.get().min(MAX_TOKIO_BLOCKING_THREADS)
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroUsize;

    use super::*;

    #[test]
    fn blocking_budget_tracks_small_hosts_and_caps_large_hosts() {
        assert_eq!(
            tokio_blocking_thread_budget_for_parallelism(NonZeroUsize::new(1).unwrap()),
            1
        );
        assert_eq!(
            tokio_blocking_thread_budget_for_parallelism(NonZeroUsize::new(3).unwrap()),
            3
        );
        assert_eq!(
            tokio_blocking_thread_budget_for_parallelism(NonZeroUsize::new(4).unwrap()),
            4
        );
        assert_eq!(
            tokio_blocking_thread_budget_for_parallelism(NonZeroUsize::new(32).unwrap()),
            4
        );
    }
}
