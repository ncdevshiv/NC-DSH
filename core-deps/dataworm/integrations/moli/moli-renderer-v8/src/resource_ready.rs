use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::Instant,
};

static NEXT_PAGE_TASK_READY_ORDER: AtomicU64 = AtomicU64::new(1);

#[derive(Debug)]
pub(crate) struct ReadyPageTask<T> {
    // Immediate queues are separate task sources. This ticket gives the Page
    // scheduler one deterministic enqueue order; it is not a Web ordering
    // guarantee. `ready_at` is retained for arbitration against delayed tasks
    // such as timers.
    pub(crate) ready_at: Instant,
    pub(crate) order: u64,
    pub(crate) value: T,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct RendererPageTaskReadyMetadata {
    pub(crate) ready_at: Instant,
    pub(crate) order: u64,
}

impl RendererPageTaskReadyMetadata {
    pub(crate) fn at(ready_at: Instant) -> Self {
        Self {
            ready_at,
            order: NEXT_PAGE_TASK_READY_ORDER.fetch_add(1, Ordering::Relaxed),
        }
    }
}

impl<T> ReadyPageTask<T> {
    pub(crate) fn new(value: T) -> Self {
        Self::at(value, Instant::now())
    }

    /// Creates a task whose source does not expose it before `ready_at`.
    ///
    /// The ticket is allocated when the delayed task is posted, while
    /// arbitration uses the instant at which it actually becomes runnable.
    pub(crate) fn at(value: T, ready_at: Instant) -> Self {
        let ready = RendererPageTaskReadyMetadata::at(ready_at);
        Self {
            ready_at: ready.ready_at,
            order: ready.order,
            value,
        }
    }

    pub(crate) fn metadata(&self) -> RendererPageTaskReadyMetadata {
        RendererPageTaskReadyMetadata {
            ready_at: self.ready_at,
            order: self.order,
        }
    }

    pub(crate) fn value(&self) -> &T {
        &self.value
    }

    pub(crate) fn into_parts(self) -> (RendererPageTaskReadyMetadata, T) {
        (self.metadata(), self.value)
    }
}
