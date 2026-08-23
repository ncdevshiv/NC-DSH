//! Pure queue, backpressure, and sink/transform pump planning for the current
//! `WritableStreamDefaultController` storage model.

use crate::queue::{QueueBounds, QueueDequeuePlan, QueueTotalPlan, QueueTotalSize};
use crate::strategy::StrategySnapshot;
use crate::writable::WritableState;

const SINK_CONTINUATION_SCHEDULED: u32 = 1 << 0;
const TRANSFORM_CONTINUATION_SCHEDULED: u32 = 1 << 1;
const TRANSFORM_CLOSE_IN_FLIGHT: u32 = 1 << 2;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PendingWriteKind {
    Transform,
    TransformRunning,
    Close,
    Sink,
    SinkRunning,
    SinkCloseRunning,
}

impl PendingWriteKind {
    #[must_use]
    pub const fn is_running(self) -> bool {
        matches!(
            self,
            Self::TransformRunning | Self::SinkRunning | Self::SinkCloseRunning
        )
    }

    #[must_use]
    pub const fn is_close(self) -> bool {
        matches!(self, Self::Close | Self::SinkCloseRunning)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PumpState(u32);

impl PumpState {
    #[must_use]
    pub const fn from_stored(value: u32) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn stored(self) -> u32 {
        self.0
    }

    #[must_use]
    pub const fn sink_continuation_scheduled(self) -> bool {
        self.0 & SINK_CONTINUATION_SCHEDULED != 0
    }

    #[must_use]
    pub const fn transform_continuation_scheduled(self) -> bool {
        self.0 & TRANSFORM_CONTINUATION_SCHEDULED != 0
    }

    #[must_use]
    pub const fn transform_close_in_flight(self) -> bool {
        self.0 & TRANSFORM_CLOSE_IN_FLIGHT != 0
    }

    #[must_use]
    pub const fn with_sink_continuation(self, scheduled: bool) -> Self {
        self.with_bit(SINK_CONTINUATION_SCHEDULED, scheduled)
    }

    #[must_use]
    pub const fn with_transform_continuation(self, scheduled: bool) -> Self {
        self.with_bit(TRANSFORM_CONTINUATION_SCHEDULED, scheduled)
    }

    #[must_use]
    pub const fn with_transform_close_in_flight(self, in_flight: bool) -> Self {
        self.with_bit(TRANSFORM_CLOSE_IN_FLIGHT, in_flight)
    }

    const fn with_bit(self, bit: u32, enabled: bool) -> Self {
        if enabled {
            Self(self.0 | bit)
        } else {
            Self(self.0 & !bit)
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WritableControllerSnapshot {
    strategy: StrategySnapshot,
    queue: QueueBounds,
    head_kind: Option<PendingWriteKind>,
    start_pending: bool,
    state: WritableState,
    close_requested: bool,
    pump: PumpState,
}

impl WritableControllerSnapshot {
    #[must_use]
    pub const fn new(
        strategy: StrategySnapshot,
        queue: QueueBounds,
        head_kind: Option<PendingWriteKind>,
        start_pending: bool,
        state: WritableState,
        close_requested: bool,
        pump: PumpState,
    ) -> Self {
        Self {
            strategy,
            queue,
            head_kind,
            start_pending,
            state,
            close_requested,
            pump,
        }
    }

    #[must_use]
    pub const fn queue(self) -> QueueBounds {
        self.queue
    }

    #[must_use]
    pub const fn pump(self) -> PumpState {
        self.pump
    }

    #[must_use]
    pub fn plan_record_write(self, size: f64) -> WriteSizePlan {
        let total =
            QueueTotalSize::from_stored(self.strategy.queue_total_size()).plan_enqueue(size);
        let next = StrategySnapshot::new(self.strategy.high_water_mark(), total.next().value());
        WriteSizePlan {
            total,
            ready: if next.desired_size() <= 0.0 {
                ReadyTransition::EnsurePending
            } else {
                ReadyTransition::Keep
            },
        }
    }

    #[must_use]
    pub fn plan_finish_write(self, size: f64, completion: WriteCompletion) -> WriteSizePlan {
        let total =
            QueueTotalSize::from_stored(self.strategy.queue_total_size()).plan_dequeue(size);
        let next = StrategySnapshot::new(self.strategy.high_water_mark(), total.next().value());
        WriteSizePlan {
            total,
            ready: if matches!(completion, WriteCompletion::Fulfilled)
                && matches!(self.state, WritableState::Writable)
                && !self.close_requested
                && next.desired_size() > 0.0
            {
                ReadyTransition::ResolvePending
            } else {
                ReadyTransition::Keep
            },
        }
    }

    #[must_use]
    pub const fn plan_sink_pump(self) -> SinkPumpPlan {
        if self.start_pending || self.pump.sink_continuation_scheduled() {
            return SinkPumpPlan::Wait;
        }
        if matches!(self.state, WritableState::Closed | WritableState::Errored) {
            return SinkPumpPlan::Wait;
        }
        if matches!(self.state, WritableState::Erroring) {
            let head_is_running = match self.head_kind {
                Some(kind) => kind.is_running(),
                None => false,
            };
            return if head_is_running {
                SinkPumpPlan::Wait
            } else {
                SinkPumpPlan::FinishErroring
            };
        }
        let Some(kind) = self.head_kind else {
            return SinkPumpPlan::Wait;
        };
        if kind.is_running() || (!kind.is_close() && !matches!(kind, PendingWriteKind::Sink)) {
            return SinkPumpPlan::Wait;
        }
        if kind.is_close() {
            SinkPumpPlan::StartClose {
                source: kind,
                running: PendingWriteKind::SinkCloseRunning,
            }
        } else {
            SinkPumpPlan::StartWrite {
                source: kind,
                running: PendingWriteKind::SinkRunning,
            }
        }
    }

    #[must_use]
    pub const fn plan_transform_pump(self, readable_has_capacity: bool) -> TransformPumpPlan {
        if self.start_pending || self.pump.transform_continuation_scheduled() {
            return TransformPumpPlan::Wait;
        }
        if matches!(self.state, WritableState::Closed | WritableState::Errored) {
            return TransformPumpPlan::Wait;
        }
        if matches!(self.state, WritableState::Erroring) {
            let head_is_running = match self.head_kind {
                Some(kind) => kind.is_running(),
                None => false,
            };
            return if self.pump.transform_close_in_flight() || head_is_running {
                TransformPumpPlan::Wait
            } else {
                TransformPumpPlan::FinishErroring
            };
        }
        let Some(kind) = self.head_kind else {
            return TransformPumpPlan::Wait;
        };
        if kind.is_running() || (!kind.is_close() && !readable_has_capacity) {
            return TransformPumpPlan::Wait;
        }
        if kind.is_close() {
            TransformPumpPlan::StartClose {
                source: kind,
                running: PendingWriteKind::TransformRunning,
            }
        } else {
            TransformPumpPlan::StartWrite {
                source: kind,
                running: PendingWriteKind::TransformRunning,
            }
        }
    }

    #[must_use]
    pub const fn plan_schedule_sink_continuation(self) -> ContinuationPlan {
        if !matches!(self.state, WritableState::Writable) || self.pump.sink_continuation_scheduled()
        {
            return ContinuationPlan::Ignore;
        }
        let Some(kind) = self.head_kind else {
            return ContinuationPlan::Ignore;
        };
        if kind.is_running() || (!kind.is_close() && !matches!(kind, PendingWriteKind::Sink)) {
            return ContinuationPlan::Ignore;
        }
        ContinuationPlan::Schedule(self.pump.with_sink_continuation(true))
    }

    #[must_use]
    pub const fn plan_schedule_transform_continuation(self) -> ContinuationPlan {
        if !matches!(self.state, WritableState::Writable)
            || self.pump.transform_continuation_scheduled()
        {
            return ContinuationPlan::Ignore;
        }
        let Some(kind) = self.head_kind else {
            return ContinuationPlan::Ignore;
        };
        if kind.is_running() {
            return ContinuationPlan::Ignore;
        }
        ContinuationPlan::Schedule(self.pump.with_transform_continuation(true))
    }

    #[must_use]
    pub const fn plan_dequeue(self) -> Option<QueueDequeuePlan> {
        self.queue.dequeue()
    }

    #[must_use]
    pub const fn plan_reject_entry(kind: PendingWriteKind) -> RejectEntryPlan {
        if kind.is_close() {
            RejectEntryPlan::DeferClosePromise
        } else if matches!(kind, PendingWriteKind::TransformRunning) {
            RejectEntryPlan::FinishWithoutRejectingPromise
        } else {
            RejectEntryPlan::FinishAndRejectPromise
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WriteCompletion {
    Fulfilled,
    Rejected,
    Discarded,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WriteSizePlan {
    total: QueueTotalPlan,
    ready: ReadyTransition,
}

impl WriteSizePlan {
    #[must_use]
    pub const fn total(self) -> QueueTotalPlan {
        self.total
    }

    #[must_use]
    pub const fn ready(self) -> ReadyTransition {
        self.ready
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReadyTransition {
    Keep,
    EnsurePending,
    ResolvePending,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SinkPumpPlan {
    Wait,
    FinishErroring,
    StartWrite {
        source: PendingWriteKind,
        running: PendingWriteKind,
    },
    StartClose {
        source: PendingWriteKind,
        running: PendingWriteKind,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransformPumpPlan {
    Wait,
    FinishErroring,
    StartWrite {
        source: PendingWriteKind,
        running: PendingWriteKind,
    },
    StartClose {
        source: PendingWriteKind,
        running: PendingWriteKind,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContinuationPlan {
    Ignore,
    Schedule(PumpState),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RejectEntryPlan {
    FinishAndRejectPromise,
    FinishWithoutRejectingPromise,
    DeferClosePromise,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(kind: Option<PendingWriteKind>) -> WritableControllerSnapshot {
        WritableControllerSnapshot::new(
            StrategySnapshot::new(2.0, 0.5),
            QueueBounds::new(0, usize::from(kind.is_some())).unwrap(),
            kind,
            false,
            WritableState::Writable,
            false,
            PumpState::from_stored(0),
        )
    }

    #[test]
    fn write_size_plans_pair_total_size_and_ready_transitions() {
        let record = snapshot(None).plan_record_write(2.0);
        assert_eq!(record.total().next().value(), 2.5);
        assert_eq!(record.ready(), ReadyTransition::EnsurePending);

        let finish = WritableControllerSnapshot::new(
            StrategySnapshot::new(2.0, 2.5),
            QueueBounds::new(0, 0).unwrap(),
            None,
            false,
            WritableState::Writable,
            false,
            PumpState::from_stored(0),
        )
        .plan_finish_write(2.0, WriteCompletion::Fulfilled);
        assert_eq!(finish.total().next().value(), 0.5);
        assert_eq!(finish.ready(), ReadyTransition::ResolvePending);

        for (completion, close_requested) in [
            (WriteCompletion::Rejected, false),
            (WriteCompletion::Discarded, false),
            (WriteCompletion::Fulfilled, true),
        ] {
            let finish = WritableControllerSnapshot::new(
                StrategySnapshot::new(2.0, 2.5),
                QueueBounds::new(0, 0).unwrap(),
                None,
                false,
                WritableState::Writable,
                close_requested,
                PumpState::from_stored(0),
            )
            .plan_finish_write(2.0, completion);
            assert_eq!(finish.total().next().value(), 0.5);
            assert_eq!(finish.ready(), ReadyTransition::Keep);
        }
    }

    #[test]
    fn sink_and_transform_pumps_partition_head_kinds() {
        assert_eq!(snapshot(None).plan_sink_pump(), SinkPumpPlan::Wait);
        assert_eq!(
            snapshot(Some(PendingWriteKind::Sink)).plan_sink_pump(),
            SinkPumpPlan::StartWrite {
                source: PendingWriteKind::Sink,
                running: PendingWriteKind::SinkRunning,
            }
        );
        assert_eq!(
            snapshot(Some(PendingWriteKind::Close)).plan_sink_pump(),
            SinkPumpPlan::StartClose {
                source: PendingWriteKind::Close,
                running: PendingWriteKind::SinkCloseRunning,
            }
        );
        assert_eq!(
            snapshot(Some(PendingWriteKind::Transform)).plan_transform_pump(false),
            TransformPumpPlan::Wait
        );
        assert_eq!(
            snapshot(Some(PendingWriteKind::Transform)).plan_transform_pump(true),
            TransformPumpPlan::StartWrite {
                source: PendingWriteKind::Transform,
                running: PendingWriteKind::TransformRunning,
            }
        );
    }

    #[test]
    fn pump_bits_and_rejection_ownership_are_explicit() {
        let initial = PumpState::from_stored(0);
        let sink = initial.with_sink_continuation(true);
        let transform = sink.with_transform_continuation(true);
        let close = transform.with_transform_close_in_flight(true);
        assert!(sink.sink_continuation_scheduled());
        assert!(transform.transform_continuation_scheduled());
        assert!(close.transform_close_in_flight());
        assert_eq!(close.with_sink_continuation(false).stored(), 0b110);
        assert_eq!(
            WritableControllerSnapshot::plan_reject_entry(PendingWriteKind::TransformRunning),
            RejectEntryPlan::FinishWithoutRejectingPromise
        );
        assert_eq!(
            WritableControllerSnapshot::plan_reject_entry(PendingWriteKind::Close),
            RejectEntryPlan::DeferClosePromise
        );
    }

    #[test]
    fn pump_blockers_and_continuations_cover_every_head_kind() {
        let kinds = [
            PendingWriteKind::Transform,
            PendingWriteKind::TransformRunning,
            PendingWriteKind::Close,
            PendingWriteKind::Sink,
            PendingWriteKind::SinkRunning,
            PendingWriteKind::SinkCloseRunning,
        ];
        for kind in kinds {
            let current = snapshot(Some(kind));
            let sink_can_start = matches!(kind, PendingWriteKind::Close | PendingWriteKind::Sink);
            assert_eq!(
                current.plan_sink_pump() != SinkPumpPlan::Wait,
                sink_can_start
            );
            assert_eq!(
                current.plan_schedule_sink_continuation() != ContinuationPlan::Ignore,
                sink_can_start
            );
            assert_eq!(
                current.plan_transform_pump(true) != TransformPumpPlan::Wait,
                !kind.is_running()
            );
            assert_eq!(
                current.plan_schedule_transform_continuation() != ContinuationPlan::Ignore,
                !kind.is_running()
            );
        }

        let blocked = WritableControllerSnapshot::new(
            StrategySnapshot::new(1.0, 0.0),
            QueueBounds::new(0, 1).unwrap(),
            Some(PendingWriteKind::Sink),
            true,
            WritableState::Erroring,
            false,
            PumpState::from_stored(0),
        );
        assert_eq!(blocked.plan_sink_pump(), SinkPumpPlan::Wait);
        assert_eq!(blocked.plan_transform_pump(true), TransformPumpPlan::Wait);
    }

    #[test]
    fn erroring_finishes_only_after_the_running_entry_clears() {
        let idle = WritableControllerSnapshot::new(
            StrategySnapshot::new(1.0, 0.0),
            QueueBounds::new(0, 0).unwrap(),
            None,
            false,
            WritableState::Erroring,
            false,
            PumpState::from_stored(0),
        );
        assert_eq!(idle.plan_sink_pump(), SinkPumpPlan::FinishErroring);
        assert_eq!(
            idle.plan_transform_pump(true),
            TransformPumpPlan::FinishErroring
        );

        let running = WritableControllerSnapshot::new(
            StrategySnapshot::new(1.0, 0.0),
            QueueBounds::new(0, 1).unwrap(),
            Some(PendingWriteKind::SinkRunning),
            false,
            WritableState::Erroring,
            false,
            PumpState::from_stored(0),
        );
        assert_eq!(running.plan_sink_pump(), SinkPumpPlan::Wait);
    }
}
