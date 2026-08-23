//! Runtime-independent queue index and total-size planning.
//!
//! Queue payloads and storage identity stay in the runtime adapter. These
//! types only describe the live range and the arithmetic that a successful
//! storage commit must apply.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QueueBoundsError {
    HeadPastEnd,
}

/// The live portion of an adapter-owned queue storage object.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QueueBounds {
    head: usize,
    storage_len: usize,
}

impl QueueBounds {
    pub fn new(head: usize, storage_len: usize) -> Result<Self, QueueBoundsError> {
        if head > storage_len {
            return Err(QueueBoundsError::HeadPastEnd);
        }
        Ok(Self { head, storage_len })
    }

    #[must_use]
    pub const fn head(self) -> usize {
        self.head
    }

    #[must_use]
    pub const fn storage_len(self) -> usize {
        self.storage_len
    }

    #[must_use]
    pub const fn live_len(self) -> usize {
        self.storage_len - self.head
    }

    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.head == self.storage_len
    }

    #[must_use]
    pub const fn append_index(self) -> usize {
        self.storage_len
    }

    #[must_use]
    pub const fn dequeue(self) -> Option<QueueDequeuePlan> {
        if self.is_empty() {
            return None;
        }
        let next_head = self.head + 1;
        let remainder = if next_head == self.storage_len {
            QueueRemainderPlan::Reset
        } else {
            QueueRemainderPlan::AdvanceHead(next_head)
        };
        Some(QueueDequeuePlan {
            index: self.head,
            remainder,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QueueDequeuePlan {
    index: usize,
    remainder: QueueRemainderPlan,
}

impl QueueDequeuePlan {
    #[must_use]
    pub const fn index(self) -> usize {
        self.index
    }

    #[must_use]
    pub const fn remainder(self) -> QueueRemainderPlan {
        self.remainder
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QueueRemainderPlan {
    Reset,
    AdvanceHead(usize),
}

/// The queue total stored by a controller or derived from entry metadata.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct QueueTotalSize(f64);

impl QueueTotalSize {
    #[must_use]
    pub const fn from_stored(value: f64) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn value(self) -> f64 {
        self.0
    }

    /// Adds one already-validated queue entry size while preserving native
    /// double arithmetic.
    #[must_use]
    pub fn accumulate(self, entry_size: f64) -> Self {
        Self(self.0 + entry_size)
    }

    /// Mirrors the Streams queue-total guard used after subtraction.
    #[must_use]
    pub fn clamp_non_negative(self) -> Self {
        if self.0 < 0.0 { Self(0.0) } else { self }
    }

    #[must_use]
    pub fn plan_enqueue(self, entry_size: f64) -> QueueTotalPlan {
        QueueTotalPlan {
            source: self,
            entry_size,
            next: self.accumulate(entry_size),
        }
    }

    #[must_use]
    pub fn plan_dequeue(self, entry_size: f64) -> QueueTotalPlan {
        QueueTotalPlan {
            source: self,
            entry_size,
            // Preserve the adapter's previous `(total - size).max(0.0)`
            // boundary exactly. Besides clamping a negative remainder, f64
            // `max` canonicalizes negative zero and the renderer's current
            // internal missing-size NaN sentinel to positive zero.
            next: Self((self.0 - entry_size).max(0.0)),
        }
    }
}

/// A total-size transition. The adapter commits `next` only against the same
/// live storage generation from which `source` was observed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct QueueTotalPlan {
    source: QueueTotalSize,
    entry_size: f64,
    next: QueueTotalSize,
}

impl QueueTotalPlan {
    #[must_use]
    pub const fn source(self) -> QueueTotalSize {
        self.source
    }

    #[must_use]
    pub const fn entry_size(self) -> f64 {
        self.entry_size
    }

    #[must_use]
    pub const fn next(self) -> QueueTotalSize {
        self.next
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queue_bounds_reject_invalid_heads_and_describe_the_live_range() {
        assert_eq!(QueueBounds::new(4, 3), Err(QueueBoundsError::HeadPastEnd));

        let bounds = QueueBounds::new(2, 5).expect("valid queue bounds");
        assert_eq!(bounds.head(), 2);
        assert_eq!(bounds.storage_len(), 5);
        assert_eq!(bounds.live_len(), 3);
        assert_eq!(bounds.append_index(), 5);
        assert!(!bounds.is_empty());
    }

    #[test]
    fn dequeue_plans_advance_or_reset_without_underflow() {
        assert_eq!(QueueBounds::new(0, 0).expect("empty queue").dequeue(), None);
        assert_eq!(
            QueueBounds::new(1, 3).expect("two live entries").dequeue(),
            Some(QueueDequeuePlan {
                index: 1,
                remainder: QueueRemainderPlan::AdvanceHead(2),
            })
        );
        assert_eq!(
            QueueBounds::new(2, 3).expect("last live entry").dequeue(),
            Some(QueueDequeuePlan {
                index: 2,
                remainder: QueueRemainderPlan::Reset,
            })
        );
    }

    #[test]
    fn total_size_plans_preserve_current_double_and_clamp_semantics() {
        let start = QueueTotalSize::from_stored(1e-16);
        let enqueue = start.plan_enqueue(1.0);
        assert_eq!(enqueue.source(), start);
        assert_eq!(enqueue.entry_size(), 1.0);
        assert_eq!(enqueue.next().value(), 1e-16 + 1.0);

        let dequeue = enqueue.next().plan_dequeue(1e-16);
        assert_eq!(dequeue.next().value(), 1e-16 + 1.0 - 1e-16);

        let overdraw = QueueTotalSize::from_stored(1.0).plan_dequeue(2.0);
        assert_eq!(overdraw.next().value(), 0.0);

        let missing_size = QueueTotalSize::from_stored(1.0).plan_dequeue(f64::NAN);
        assert_eq!(missing_size.next().value().to_bits(), 0.0f64.to_bits());

        let negative_zero = QueueTotalSize::from_stored(-0.0).plan_dequeue(0.0);
        assert_eq!(negative_zero.next().value().to_bits(), 0.0f64.to_bits());
    }

    #[test]
    fn accumulating_entry_metadata_clamps_only_when_requested() {
        let total = QueueTotalSize::default()
            .accumulate(2.0)
            .accumulate(9_007_199_254_740_991.0);
        assert_eq!(total.value(), 0.0 + 2.0 + 9_007_199_254_740_991.0);
        assert_eq!(
            QueueTotalSize::from_stored(-1.0)
                .clamp_non_negative()
                .value(),
            0.0
        );
    }
}
