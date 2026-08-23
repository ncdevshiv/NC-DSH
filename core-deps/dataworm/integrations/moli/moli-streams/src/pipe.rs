//! Runtime-independent `ReadableStreamPipeTo` coordination.
//!
//! JavaScript promises, chunks, errors, AbortSignal listeners, and stream
//! wrappers remain adapter-owned. This module owns pipe lifecycle and joins
//! narrow stream observations at entry, drain, terminal, and shutdown-action
//! decision boundaries.

use crate::readable::ReadableState;
use crate::writable::WritableState;
use std::convert::Infallible;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PipeOptions {
    prevent_close: bool,
    prevent_abort: bool,
    prevent_cancel: bool,
}

impl PipeOptions {
    const PREVENT_CLOSE: u8 = 1 << 0;
    const PREVENT_ABORT: u8 = 1 << 1;
    const PREVENT_CANCEL: u8 = 1 << 2;
    const VALID_BITS: u8 = Self::PREVENT_CLOSE | Self::PREVENT_ABORT | Self::PREVENT_CANCEL;

    #[must_use]
    pub const fn new(prevent_close: bool, prevent_abort: bool, prevent_cancel: bool) -> Self {
        Self {
            prevent_close,
            prevent_abort,
            prevent_cancel,
        }
    }

    #[must_use]
    pub const fn bits(self) -> u8 {
        (if self.prevent_close {
            Self::PREVENT_CLOSE
        } else {
            0
        }) | (if self.prevent_abort {
            Self::PREVENT_ABORT
        } else {
            0
        }) | (if self.prevent_cancel {
            Self::PREVENT_CANCEL
        } else {
            0
        })
    }

    #[must_use]
    pub const fn from_bits(bits: u8) -> Option<Self> {
        if bits & !Self::VALID_BITS != 0 {
            return None;
        }
        Some(Self::new(
            bits & Self::PREVENT_CLOSE != 0,
            bits & Self::PREVENT_ABORT != 0,
            bits & Self::PREVENT_CANCEL != 0,
        ))
    }
}

/// The part of shutdown currently owned by the pipe operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PipeShutdownStage {
    WaitingForWritePublication,
    WaitingForLastWrite,
    RunningActions,
    Finalizing,
}

/// Barrier selected when a terminal event races the pipe write hot path.
/// A write callback can synchronously re-enter the source before its returned
/// promise has been published into the pipe owner.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PipeWriteBarrier {
    None,
    WaitForPublication,
    WaitForSettlement,
}

/// The first terminal condition which claimed a pipe.
///
/// The ordering of these events is observable.  For example, an initially
/// errored source wins over an initially errored destination because the
/// PipeTo algorithm checks the source first.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PipeTerminalTrigger {
    SourceErrored,
    DestinationErrored,
    SourceClosed,
    DestinationClosed,
    Aborted,
}

/// Authoritative lifecycle of one `ReadableStreamPipeTo` operation.
///
/// The terminal trigger exists only while shutdown owns asynchronous work;
/// impossible combinations such as an active pipe with a shutdown stage are
/// not representable.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PipeLifecycle {
    Active,
    ShuttingDown {
        stage: PipeShutdownStage,
        trigger: PipeTerminalTrigger,
    },
    Finished,
}

/// The asynchronous action performed after a terminal condition is claimed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PipeShutdownAction {
    AbortDestination,
    CancelSource,
    CloseDestination,
    AbortDestinationAndCancelSource,
}

/// One member of a possibly joined shutdown action. The destination action is
/// first in the specification's Promise.all input list and therefore owns the
/// rejection reason when both members reject, regardless of callback order.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PipeShutdownOperation {
    Destination,
    Source,
}

impl PipeShutdownOperation {
    const fn bit(self) -> u8 {
        match self {
            Self::Destination => 1 << 0,
            Self::Source => 1 << 1,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PipeShutdownSettlements {
    bits: u8,
}

impl PipeShutdownSettlements {
    const OPERATION_MASK: u8 =
        PipeShutdownOperation::Destination.bit() | PipeShutdownOperation::Source.bit();
    const REJECTED_SHIFT: u8 = 2;
    const VALID_BITS: u8 = Self::OPERATION_MASK | (Self::OPERATION_MASK << Self::REJECTED_SHIFT);

    #[must_use]
    pub const fn from_bits(bits: u8) -> Option<Self> {
        if bits & !Self::VALID_BITS != 0 {
            return None;
        }
        let value = Self { bits };
        if value.rejected_mask() & !value.settled_mask() != 0 {
            return None;
        }
        Some(value)
    }

    #[must_use]
    pub const fn bits(self) -> u8 {
        self.bits
    }

    const fn settled_mask(self) -> u8 {
        self.bits & Self::OPERATION_MASK
    }

    const fn rejected_mask(self) -> u8 {
        (self.bits >> Self::REJECTED_SHIFT) & Self::OPERATION_MASK
    }

    const fn is_empty(self) -> bool {
        self.bits == 0
    }

    const fn is_settled(self, operation: PipeShutdownOperation) -> bool {
        self.settled_mask() & operation.bit() != 0
    }

    const fn is_rejected(self, operation: PipeShutdownOperation) -> bool {
        self.rejected_mask() & operation.bit() != 0
    }

    const fn settle(self, operation: PipeShutdownOperation, rejected: bool) -> Self {
        let mut bits = self.bits | operation.bit();
        if rejected {
            bits |= operation.bit() << Self::REJECTED_SHIFT;
        }
        Self { bits }
    }
}

impl PipeShutdownAction {
    const fn operation_mask(self) -> u8 {
        match self {
            Self::AbortDestination | Self::CloseDestination => {
                PipeShutdownOperation::Destination.bit()
            }
            Self::CancelSource => PipeShutdownOperation::Source.bit(),
            Self::AbortDestinationAndCancelSource => {
                PipeShutdownOperation::Destination.bit() | PipeShutdownOperation::Source.bit()
            }
        }
    }
}

/// The exact source of the pipe operation's final settlement.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PipeFinalSettlement {
    Resolve,
    RejectOriginal,
    RejectShutdownAction { operation: PipeShutdownOperation },
}

/// Required AbortSignal registration cleanup at finalization.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PipeAbortCleanup {
    None,
    Unregister,
    ClearDispatching,
}

/// Complete adapter-owned work required to finalize one pipe operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PipeFinalization {
    settlement: PipeFinalSettlement,
    abort_cleanup: PipeAbortCleanup,
}

impl PipeFinalization {
    #[must_use]
    pub const fn settlement(self) -> PipeFinalSettlement {
        self.settlement
    }

    #[must_use]
    pub const fn abort_cleanup(self) -> PipeAbortCleanup {
        self.abort_cleanup
    }
}

/// Primitive state stored by the V8-resident pipe owner.
///
/// V8 references such as the streams, resolver, last-write promise, and error
/// value deliberately remain in the renderer adapter.  Every primitive which
/// decides whether a callback may mutate those references lives here.
///
/// One owner is created for exactly one pipe operation and is never reused:
/// `Active -> ShuttingDown -> Finished` is a one-way transition. Callbacks
/// retain that owner identity directly, so the tagged lifecycle and
/// operation-settlement bits reject stale work without a numeric generation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PipeOwnerState {
    lifecycle: PipeLifecycle,
    options: PipeOptions,
    drain_scheduled: bool,
    read_pending: bool,
    write_in_progress: bool,
    drain_yield_once: bool,
    abort_listener: AbortListenerState,
    shutdown_settlements: PipeShutdownSettlements,
}

impl PipeOwnerState {
    #[must_use]
    pub const fn new(options: PipeOptions) -> Self {
        Self {
            lifecycle: PipeLifecycle::Active,
            options,
            drain_scheduled: false,
            read_pending: false,
            write_in_progress: false,
            drain_yield_once: false,
            abort_listener: AbortListenerState::None,
            shutdown_settlements: PipeShutdownSettlements { bits: 0 },
        }
    }

    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn from_storage(
        lifecycle: PipeLifecycle,
        options: PipeOptions,
        drain_scheduled: bool,
        read_pending: bool,
        write_in_progress: bool,
        drain_yield_once: bool,
        abort_listener: AbortListenerState,
        shutdown_settlements: PipeShutdownSettlements,
    ) -> Option<Self> {
        let state = Self {
            lifecycle,
            options,
            drain_scheduled,
            read_pending,
            write_in_progress,
            drain_yield_once,
            abort_listener,
            shutdown_settlements,
        };
        if state.storage_invariants_hold() {
            Some(state)
        } else {
            None
        }
    }

    #[must_use]
    pub const fn lifecycle(self) -> PipeLifecycle {
        self.lifecycle
    }

    #[must_use]
    pub const fn drain_scheduled(self) -> bool {
        self.drain_scheduled
    }

    #[must_use]
    pub const fn read_pending(self) -> bool {
        self.read_pending
    }

    #[must_use]
    pub const fn write_in_progress(self) -> bool {
        self.write_in_progress
    }

    #[must_use]
    pub const fn drain_yield_once(self) -> bool {
        self.drain_yield_once
    }

    #[must_use]
    pub const fn abort_listener(self) -> AbortListenerState {
        self.abort_listener
    }

    #[must_use]
    pub const fn terminal_trigger(self) -> Option<PipeTerminalTrigger> {
        match self.lifecycle {
            PipeLifecycle::ShuttingDown { trigger, .. } => Some(trigger),
            PipeLifecycle::Active | PipeLifecycle::Finished => None,
        }
    }

    #[must_use]
    pub const fn shutdown_settlements(self) -> PipeShutdownSettlements {
        self.shutdown_settlements
    }

    #[must_use]
    pub const fn is_active(self) -> bool {
        matches!(self.lifecycle, PipeLifecycle::Active)
    }

    #[must_use]
    pub const fn is_waiting_for_write_publication(self) -> bool {
        matches!(
            self.lifecycle,
            PipeLifecycle::ShuttingDown {
                stage: PipeShutdownStage::WaitingForWritePublication,
                ..
            }
        )
    }

    const fn storage_invariants_hold(self) -> bool {
        match self.lifecycle {
            PipeLifecycle::Active => self.shutdown_settlements.is_empty(),
            PipeLifecycle::ShuttingDown { stage, .. } => {
                if self.drain_scheduled || self.read_pending || self.drain_yield_once {
                    return false;
                }
                match stage {
                    PipeShutdownStage::WaitingForWritePublication => {
                        self.write_in_progress && self.shutdown_settlements.is_empty()
                    }
                    PipeShutdownStage::WaitingForLastWrite => {
                        !self.write_in_progress && self.shutdown_settlements.is_empty()
                    }
                    PipeShutdownStage::RunningActions => {
                        let required = self.required_shutdown_action_mask();
                        !self.write_in_progress
                            && required != 0
                            && self.shutdown_settlements.settled_mask() & !required == 0
                            && self.remaining_shutdown_action_mask() != 0
                    }
                    PipeShutdownStage::Finalizing => {
                        let required = self.required_shutdown_action_mask();
                        !self.write_in_progress
                            && required != 0
                            && self.shutdown_settlements.settled_mask() == required
                            && self.shutdown_settlements.rejected_mask() == 0
                    }
                }
            }
            PipeLifecycle::Finished => {
                !self.drain_scheduled
                    && !self.read_pending
                    && !self.write_in_progress
                    && !self.drain_yield_once
                    && matches!(self.abort_listener, AbortListenerState::None)
                    && self.shutdown_settlements.is_empty()
            }
        }
    }

    #[must_use]
    pub const fn plan_incoming_chunk(self) -> PipeIncomingChunkPlan {
        if !self.is_active() {
            PipeIncomingChunkPlan::NotPiped
        } else if self.read_pending {
            PipeIncomingChunkPlan::EnqueueAndSchedule {
                size: PipeChunkSize::Zero,
            }
        } else {
            PipeIncomingChunkPlan::EnqueueAndSchedule {
                size: PipeChunkSize::Strategy,
            }
        }
    }

    #[must_use]
    pub const fn has_pull_demand(self, source_has_capacity: bool) -> bool {
        // Destination capacity controls whether PipeTo acquires a read request
        // and is therefore already represented by `read_pending`. Independently,
        // the readable controller may pull into its own strategy queue while
        // the source still has capacity.
        self.is_active() && (self.read_pending || source_has_capacity)
    }

    #[must_use]
    pub const fn plan_before_pull(self, observation: PipePullObservation) -> PipePullPlan {
        if !self.is_active() {
            return PipePullPlan::Stop;
        }
        if !matches!(observation.source_state, ReadableState::Readable) {
            return PipePullPlan::Continue;
        }
        if !observation.destination_has_capacity {
            return PipePullPlan::BlockedByDestination;
        }
        if observation.source_queue_empty {
            PipePullPlan::MarkReadPendingAndPull
        } else {
            PipePullPlan::Continue
        }
    }

    #[must_use]
    pub const fn plan_initial_terminal(
        self,
        observation: PipeEndpointObservation,
    ) -> PipeInitialTerminalPlan {
        if !self.is_active() {
            return PipeInitialTerminalPlan::Stop;
        }
        if matches!(observation.source_state, ReadableState::Errored) {
            return PipeInitialTerminalPlan::Claim(PipeEndpointTerminalTrigger::SourceErrored);
        }
        if matches!(observation.destination.state, WritableState::Errored) {
            return PipeInitialTerminalPlan::Claim(PipeEndpointTerminalTrigger::DestinationErrored);
        }
        if matches!(observation.source_state, ReadableState::Closed) {
            return PipeInitialTerminalPlan::Claim(PipeEndpointTerminalTrigger::SourceClosed);
        }
        if observation.destination.close_requested
            || matches!(observation.destination.state, WritableState::Closed)
        {
            return PipeInitialTerminalPlan::Claim(PipeEndpointTerminalTrigger::DestinationClosed);
        }
        PipeInitialTerminalPlan::Continue
    }

    #[must_use]
    pub const fn plan_shutdown_operation(
        self,
        observation: PipeShutdownOperationObservation,
    ) -> PipeShutdownOperationPlan {
        match observation {
            PipeShutdownOperationObservation::AbortDestination { destination_state }
                if matches!(self.terminal_trigger(), Some(PipeTerminalTrigger::Aborted))
                    && !matches!(destination_state, WritableState::Writable) =>
            {
                PipeShutdownOperationPlan::FulfillWithoutRunning
            }
            PipeShutdownOperationObservation::CancelSource { source_state }
                if !matches!(source_state, ReadableState::Readable) =>
            {
                PipeShutdownOperationPlan::FulfillWithoutRunning
            }
            PipeShutdownOperationObservation::AbortDestination { .. }
            | PipeShutdownOperationObservation::CancelSource { .. } => {
                PipeShutdownOperationPlan::Run
            }
        }
    }

    #[must_use]
    pub const fn transition_mutation(
        self,
        mutation: PipeOwnerMutation,
    ) -> PipeTransition<Infallible> {
        match mutation {
            PipeOwnerMutation::MarkReadPending => {
                if !self.is_active() {
                    return PipeTransition::ignored(self);
                }
                let mut next = self;
                next.read_pending = true;
                PipeTransition::applied(self, next, None)
            }
            PipeOwnerMutation::IncomingChunk => {
                if !self.is_active() {
                    return PipeTransition::ignored(self);
                }
                let mut next = self;
                next.read_pending = false;
                PipeTransition::applied(self, next, None)
            }
            PipeOwnerMutation::BeginWrite => self.begin_write(),
            PipeOwnerMutation::AbandonWrite => self.abandon_write(),
            PipeOwnerMutation::SetDrainBarrier => {
                if !self.is_active() {
                    return PipeTransition::ignored(self);
                }
                let mut next = self;
                next.drain_yield_once = true;
                PipeTransition::applied(self, next, None)
            }
            PipeOwnerMutation::ClearDrainBarrier => {
                if !self.is_active() {
                    return PipeTransition::ignored(self);
                }
                let mut next = self;
                next.drain_yield_once = false;
                PipeTransition::applied(self, next, None)
            }
            PipeOwnerMutation::AbortListenerRegistered => {
                if !self.is_active() {
                    return PipeTransition::ignored(self);
                }
                let mut next = self;
                next.abort_listener = AbortListenerState::Registered;
                PipeTransition::applied(self, next, None)
            }
            PipeOwnerMutation::AbortDispatchStarted => {
                if !self.is_active() {
                    return PipeTransition::ignored(self);
                }
                let mut next = self;
                next.abort_listener = AbortListenerState::Aborting;
                PipeTransition::applied(self, next, None)
            }
        }
    }

    #[must_use]
    pub const fn transition_drain(self, event: PipeDrainEvent) -> PipeTransition<PipeDrainCommand> {
        match event {
            PipeDrainEvent::Schedule => self.schedule_drain(),
            PipeDrainEvent::Callback => self.begin_drain(),
        }
    }

    #[must_use]
    pub const fn transition_shutdown(
        self,
        event: PipeShutdownEvent,
    ) -> PipeTransition<PipeShutdownCommand> {
        match event {
            PipeShutdownEvent::ClaimTerminal {
                trigger,
                destination,
            } => self.claim_terminal(trigger, self.write_barrier(destination)),
            PipeShutdownEvent::LastWriteSettled => self.last_write_settled(),
            PipeShutdownEvent::ActionFulfilled { operation } => {
                self.shutdown_action_settled(operation, false)
            }
            PipeShutdownEvent::ActionRejected { operation } => {
                self.shutdown_action_settled(operation, true)
            }
            PipeShutdownEvent::Finalize => self.finalize_original(),
        }
    }

    const fn schedule_drain(self) -> PipeTransition<PipeDrainCommand> {
        if !self.is_active() {
            return PipeTransition::ignored(self);
        }
        if self.drain_scheduled {
            return PipeTransition::applied(self, self, None);
        }
        let mut next = self;
        next.drain_scheduled = true;
        PipeTransition::applied(self, next, Some(PipeDrainCommand::Enqueue))
    }

    const fn begin_drain(self) -> PipeTransition<PipeDrainCommand> {
        if !self.is_active() || !self.drain_scheduled {
            return PipeTransition::ignored(self);
        }
        let mut next = self;
        if self.drain_yield_once {
            next.drain_yield_once = false;
            return PipeTransition::applied(self, next, Some(PipeDrainCommand::Enqueue));
        }
        next.drain_scheduled = false;
        PipeTransition::applied(self, next, Some(PipeDrainCommand::Run))
    }

    const fn begin_write(self) -> PipeTransition<Infallible> {
        if !self.is_active() || self.write_in_progress {
            return PipeTransition::ignored(self);
        }
        let mut next = self;
        next.write_in_progress = true;
        PipeTransition::applied(self, next, None)
    }

    #[must_use]
    pub const fn publish_last_write(self) -> PipeTransition<PipeWritePublicationCommand> {
        if !self.write_in_progress {
            return PipeTransition::ignored(self);
        }
        let mut next = self;
        next.write_in_progress = false;
        if self.is_active() {
            return PipeTransition::applied(
                self,
                next,
                Some(PipeWritePublicationCommand::StoreLastWrite),
            );
        }
        if let PipeLifecycle::ShuttingDown {
            stage: PipeShutdownStage::WaitingForWritePublication,
            trigger,
        } = self.lifecycle
        {
            next.lifecycle = PipeLifecycle::ShuttingDown {
                stage: PipeShutdownStage::WaitingForLastWrite,
                trigger,
            };
            return PipeTransition::applied(
                self,
                next,
                Some(PipeWritePublicationCommand::StoreLastWriteAndWait),
            );
        }
        PipeTransition::applied(self, next, None)
    }

    const fn abandon_write(self) -> PipeTransition<Infallible> {
        if !self.write_in_progress || self.is_waiting_for_write_publication() {
            return PipeTransition::ignored(self);
        }
        let mut next = self;
        next.write_in_progress = false;
        PipeTransition::applied(self, next, None)
    }

    const fn write_barrier(self, destination: PipeDestinationObservation) -> PipeWriteBarrier {
        if !matches!(destination.state, WritableState::Writable) || destination.close_requested {
            PipeWriteBarrier::None
        } else if self.write_in_progress {
            PipeWriteBarrier::WaitForPublication
        } else {
            // PipeTo chains shutdown from WriteQueuedChunks while the
            // destination remains writable. With no prior write this is a
            // resolved promise and therefore still a required microtask barrier.
            PipeWriteBarrier::WaitForSettlement
        }
    }

    const fn claim_terminal(
        self,
        trigger: PipeTerminalTrigger,
        write_barrier: PipeWriteBarrier,
    ) -> PipeTransition<PipeShutdownCommand> {
        if !self.is_active() {
            return PipeTransition::ignored(self);
        }
        let action = self.shutdown_action(trigger);
        let settlement = original_settlement(trigger);
        let mut next = self;
        next.drain_scheduled = false;
        next.read_pending = false;
        next.drain_yield_once = false;
        let stage = match write_barrier {
            PipeWriteBarrier::None => PipeShutdownStage::RunningActions,
            PipeWriteBarrier::WaitForPublication => PipeShutdownStage::WaitingForWritePublication,
            PipeWriteBarrier::WaitForSettlement => PipeShutdownStage::WaitingForLastWrite,
        };
        next.lifecycle = PipeLifecycle::ShuttingDown { stage, trigger };
        next.shutdown_settlements = PipeShutdownSettlements { bits: 0 };
        if !matches!(write_barrier, PipeWriteBarrier::WaitForPublication) {
            next.write_in_progress = false;
        }
        match write_barrier {
            PipeWriteBarrier::WaitForPublication => {
                return PipeTransition::applied(self, next, None);
            }
            PipeWriteBarrier::WaitForSettlement => {
                return PipeTransition::applied(
                    self,
                    next,
                    Some(PipeShutdownCommand::WaitForLastWrite),
                );
            }
            PipeWriteBarrier::None => {}
        }
        let Some(action) = action else {
            let finalization = next.finalization(settlement);
            next.finish();
            return PipeTransition::applied(
                self,
                next,
                Some(PipeShutdownCommand::Finalize { finalization }),
            );
        };
        PipeTransition::applied(self, next, Some(PipeShutdownCommand::RunActions { action }))
    }

    const fn last_write_settled(self) -> PipeTransition<PipeShutdownCommand> {
        let PipeLifecycle::ShuttingDown {
            stage: PipeShutdownStage::WaitingForLastWrite,
            trigger,
        } = self.lifecycle
        else {
            return PipeTransition::ignored(self);
        };
        let action = self.shutdown_action(trigger);
        let settlement = original_settlement(trigger);
        let mut next = self;
        next.lifecycle = PipeLifecycle::ShuttingDown {
            stage: PipeShutdownStage::RunningActions,
            trigger,
        };
        let Some(action) = action else {
            let finalization = next.finalization(settlement);
            next.finish();
            return PipeTransition::applied(
                self,
                next,
                Some(PipeShutdownCommand::Finalize { finalization }),
            );
        };
        PipeTransition::applied(self, next, Some(PipeShutdownCommand::RunActions { action }))
    }

    const fn shutdown_action_settled(
        self,
        operation: PipeShutdownOperation,
        rejected: bool,
    ) -> PipeTransition<PipeShutdownCommand> {
        let PipeLifecycle::ShuttingDown {
            stage: PipeShutdownStage::RunningActions,
            trigger,
        } = self.lifecycle
        else {
            return PipeTransition::ignored(self);
        };
        let Some(action) = self.shutdown_action(trigger) else {
            return PipeTransition::ignored(self);
        };
        if action.operation_mask() & operation.bit() == 0
            || self.shutdown_settlements.is_settled(operation)
        {
            return PipeTransition::ignored(self);
        }
        let mut next = self;
        next.shutdown_settlements = next.shutdown_settlements.settle(operation, rejected);
        if next.remaining_shutdown_action_mask() != 0 {
            return PipeTransition::applied(self, next, None);
        }
        if next
            .shutdown_settlements
            .is_rejected(PipeShutdownOperation::Destination)
        {
            let finalization = next.finalization(PipeFinalSettlement::RejectShutdownAction {
                operation: PipeShutdownOperation::Destination,
            });
            next.finish();
            return PipeTransition::applied(
                self,
                next,
                Some(PipeShutdownCommand::Finalize { finalization }),
            );
        }
        if next
            .shutdown_settlements
            .is_rejected(PipeShutdownOperation::Source)
        {
            let finalization = next.finalization(PipeFinalSettlement::RejectShutdownAction {
                operation: PipeShutdownOperation::Source,
            });
            next.finish();
            return PipeTransition::applied(
                self,
                next,
                Some(PipeShutdownCommand::Finalize { finalization }),
            );
        }
        next.lifecycle = PipeLifecycle::ShuttingDown {
            stage: PipeShutdownStage::Finalizing,
            trigger,
        };
        PipeTransition::applied(self, next, Some(PipeShutdownCommand::EnqueueFinalize))
    }

    const fn finalize_original(self) -> PipeTransition<PipeShutdownCommand> {
        let PipeLifecycle::ShuttingDown {
            stage: PipeShutdownStage::Finalizing,
            trigger,
        } = self.lifecycle
        else {
            return PipeTransition::ignored(self);
        };
        let settlement = original_settlement(trigger);
        let mut next = self;
        let finalization = next.finalization(settlement);
        next.finish();
        PipeTransition::applied(
            self,
            next,
            Some(PipeShutdownCommand::Finalize { finalization }),
        )
    }

    const fn required_shutdown_action_mask(self) -> u8 {
        match self.terminal_trigger() {
            Some(trigger) => match self.shutdown_action(trigger) {
                Some(action) => action.operation_mask(),
                None => 0,
            },
            None => 0,
        }
    }

    const fn remaining_shutdown_action_mask(self) -> u8 {
        self.required_shutdown_action_mask() & !self.shutdown_settlements.settled_mask()
    }

    const fn shutdown_action(self, trigger: PipeTerminalTrigger) -> Option<PipeShutdownAction> {
        match trigger {
            PipeTerminalTrigger::SourceErrored if !self.options.prevent_abort => {
                Some(PipeShutdownAction::AbortDestination)
            }
            PipeTerminalTrigger::DestinationErrored | PipeTerminalTrigger::DestinationClosed
                if !self.options.prevent_cancel =>
            {
                Some(PipeShutdownAction::CancelSource)
            }
            PipeTerminalTrigger::SourceClosed if !self.options.prevent_close => {
                Some(PipeShutdownAction::CloseDestination)
            }
            PipeTerminalTrigger::Aborted
                if !self.options.prevent_abort && !self.options.prevent_cancel =>
            {
                Some(PipeShutdownAction::AbortDestinationAndCancelSource)
            }
            PipeTerminalTrigger::Aborted if !self.options.prevent_abort => {
                Some(PipeShutdownAction::AbortDestination)
            }
            PipeTerminalTrigger::Aborted if !self.options.prevent_cancel => {
                Some(PipeShutdownAction::CancelSource)
            }
            _ => None,
        }
    }

    const fn finalization(self, settlement: PipeFinalSettlement) -> PipeFinalization {
        let abort_cleanup = match self.abort_listener {
            AbortListenerState::None => PipeAbortCleanup::None,
            AbortListenerState::Registered => PipeAbortCleanup::Unregister,
            AbortListenerState::Aborting => PipeAbortCleanup::ClearDispatching,
        };
        PipeFinalization {
            settlement,
            abort_cleanup,
        }
    }

    const fn finish(&mut self) {
        self.lifecycle = PipeLifecycle::Finished;
        self.drain_scheduled = false;
        self.read_pending = false;
        self.write_in_progress = false;
        self.drain_yield_once = false;
        self.abort_listener = AbortListenerState::None;
        self.shutdown_settlements = PipeShutdownSettlements { bits: 0 };
    }
}

const fn original_settlement(trigger: PipeTerminalTrigger) -> PipeFinalSettlement {
    match trigger {
        PipeTerminalTrigger::SourceClosed => PipeFinalSettlement::Resolve,
        PipeTerminalTrigger::SourceErrored
        | PipeTerminalTrigger::DestinationErrored
        | PipeTerminalTrigger::DestinationClosed
        | PipeTerminalTrigger::Aborted => PipeFinalSettlement::RejectOriginal,
    }
}

/// Primitive owner-only mutations which cannot schedule adapter work.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PipeOwnerMutation {
    MarkReadPending,
    IncomingChunk,
    BeginWrite,
    AbandonWrite,
    SetDrainBarrier,
    ClearDrainBarrier,
    AbortListenerRegistered,
    AbortDispatchStarted,
}

/// Events owned by the drain callback boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PipeDrainEvent {
    Schedule,
    Callback,
}

/// Events owned by the terminal/shutdown boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PipeShutdownEvent {
    ClaimTerminal {
        trigger: PipeTerminalTrigger,
        destination: PipeDestinationObservation,
    },
    LastWriteSettled,
    ActionFulfilled {
        operation: PipeShutdownOperation,
    },
    ActionRejected {
        operation: PipeShutdownOperation,
    },
    Finalize,
}

/// Whether a callback still owns a valid state-machine event.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PipeAdmission {
    Applied,
    Ignored,
}

/// Commands which can only be executed by the drain scheduler/callback.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PipeDrainCommand {
    Enqueue,
    Run,
}

/// Commands which can only be executed by the shutdown owner.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PipeShutdownCommand {
    WaitForLastWrite,
    RunActions { action: PipeShutdownAction },
    EnqueueFinalize,
    Finalize { finalization: PipeFinalization },
}

/// The write-publication call site owns the returned promise and is therefore
/// the only adapter boundary allowed to execute these commands.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PipeWritePublicationCommand {
    StoreLastWrite,
    StoreLastWriteAndWait,
}

/// One admitted primitive-state commit and the runtime command that follows it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PipeTransition<C: Copy> {
    source: PipeOwnerState,
    next: PipeOwnerState,
    admission: PipeAdmission,
    command: Option<C>,
}

impl<C: Copy> PipeTransition<C> {
    const fn ignored(source: PipeOwnerState) -> Self {
        Self {
            source,
            next: source,
            admission: PipeAdmission::Ignored,
            command: None,
        }
    }

    const fn applied(source: PipeOwnerState, next: PipeOwnerState, command: Option<C>) -> Self {
        Self {
            source,
            next,
            admission: PipeAdmission::Applied,
            command,
        }
    }

    #[must_use]
    pub const fn source(self) -> PipeOwnerState {
        self.source
    }

    #[must_use]
    pub const fn next(self) -> PipeOwnerState {
        self.next
    }

    #[must_use]
    pub const fn admission(self) -> PipeAdmission {
        self.admission
    }

    #[must_use]
    pub const fn command(self) -> Option<C> {
        self.command
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AbortListenerState {
    None,
    Registered,
    Aborting,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PipeIncomingChunkPlan {
    NotPiped,
    EnqueueAndSchedule { size: PipeChunkSize },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PipeChunkSize {
    Zero,
    Strategy,
}

impl PipeChunkSize {
    #[must_use]
    pub const fn fulfills_pending_read(self) -> bool {
        matches!(self, Self::Zero)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PipePullObservation {
    source_state: ReadableState,
    source_queue_empty: bool,
    destination_has_capacity: bool,
}

impl PipePullObservation {
    #[must_use]
    pub const fn new(
        source_state: ReadableState,
        source_queue_empty: bool,
        destination_has_capacity: bool,
    ) -> Self {
        Self {
            source_state,
            source_queue_empty,
            destination_has_capacity,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PipePullPlan {
    Stop,
    BlockedByDestination,
    MarkReadPendingAndPull,
    Continue,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PipeDestinationObservation {
    state: WritableState,
    close_requested: bool,
}

impl PipeDestinationObservation {
    #[must_use]
    pub const fn new(state: WritableState, close_requested: bool) -> Self {
        Self {
            state,
            close_requested,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PipeEndpointObservation {
    source_state: ReadableState,
    destination: PipeDestinationObservation,
}

impl PipeEndpointObservation {
    #[must_use]
    pub const fn new(source_state: ReadableState, destination: PipeDestinationObservation) -> Self {
        Self {
            source_state,
            destination,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PipeInitialTerminalPlan {
    Stop,
    Continue,
    Claim(PipeEndpointTerminalTrigger),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PipeEndpointTerminalTrigger {
    SourceErrored,
    DestinationErrored,
    SourceClosed,
    DestinationClosed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PipeShutdownOperationObservation {
    AbortDestination { destination_state: WritableState },
    CancelSource { source_state: ReadableState },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PipeShutdownOperationPlan {
    Run,
    FulfillWithoutRunning,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PipeEntryObservation {
    source_locked: bool,
    destination_locked: bool,
}

impl PipeEntryObservation {
    #[must_use]
    pub const fn new(source_locked: bool, destination_locked: bool) -> Self {
        Self {
            source_locked,
            destination_locked,
        }
    }

    #[must_use]
    pub const fn plan(self) -> PipeEntryPlan {
        if self.source_locked {
            PipeEntryPlan::RejectSourceLocked
        } else if self.destination_locked {
            PipeEntryPlan::RejectDestinationLocked
        } else {
            PipeEntryPlan::Start
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PipeEntryPlan {
    Start,
    RejectSourceLocked,
    RejectDestinationLocked,
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEFAULT_OPTIONS: PipeOptions = PipeOptions::new(false, false, false);
    const WRITABLE_DESTINATION: PipeDestinationObservation =
        PipeDestinationObservation::new(WritableState::Writable, false);
    const IMMEDIATE_DESTINATION: PipeDestinationObservation =
        PipeDestinationObservation::new(WritableState::Closed, false);

    fn finished_state() -> PipeOwnerState {
        PipeOwnerState::new(PipeOptions::new(true, true, true))
            .transition_shutdown(PipeShutdownEvent::ClaimTerminal {
                trigger: PipeTerminalTrigger::SourceClosed,
                destination: IMMEDIATE_DESTINATION,
            })
            .next()
    }

    const fn pull_observation(
        source_state: ReadableState,
        source_queue_empty: bool,
        destination_has_capacity: bool,
    ) -> PipePullObservation {
        PipePullObservation::new(source_state, source_queue_empty, destination_has_capacity)
    }

    #[test]
    fn schedule_and_pull_demand_require_one_active_unscheduled_pipe() {
        let active = PipeOwnerState::new(DEFAULT_OPTIONS);
        let scheduled = active.transition_drain(PipeDrainEvent::Schedule);
        assert_eq!(scheduled.command(), Some(PipeDrainCommand::Enqueue));
        assert_eq!(
            scheduled
                .next()
                .transition_drain(PipeDrainEvent::Schedule)
                .command(),
            None
        );
        assert!(active.has_pull_demand(true));
        assert!(!active.has_pull_demand(false));

        let pending = active
            .transition_mutation(PipeOwnerMutation::MarkReadPending)
            .next();
        assert!(pending.has_pull_demand(false));

        let finished = finished_state();
        assert_eq!(
            finished
                .transition_drain(PipeDrainEvent::Schedule)
                .admission(),
            PipeAdmission::Ignored
        );
        assert!(!finished.has_pull_demand(true));
    }

    #[test]
    fn incoming_pipe_chunk_consumes_only_the_owned_internal_read() {
        let active = PipeOwnerState::new(DEFAULT_OPTIONS);
        let pending = active
            .transition_mutation(PipeOwnerMutation::MarkReadPending)
            .next();
        assert_eq!(
            pending.plan_incoming_chunk(),
            PipeIncomingChunkPlan::EnqueueAndSchedule {
                size: PipeChunkSize::Zero,
            }
        );
        assert!(PipeChunkSize::Zero.fulfills_pending_read());
        assert!(!PipeChunkSize::Strategy.fulfills_pending_read());
        assert_eq!(
            active.plan_incoming_chunk(),
            PipeIncomingChunkPlan::EnqueueAndSchedule {
                size: PipeChunkSize::Strategy,
            }
        );
        assert_eq!(
            finished_state().plan_incoming_chunk(),
            PipeIncomingChunkPlan::NotPiped
        );
    }

    #[test]
    fn drain_admission_partitions_backpressure_and_pull() {
        let active = PipeOwnerState::new(DEFAULT_OPTIONS);
        assert_eq!(
            active.plan_before_pull(pull_observation(ReadableState::Readable, false, true)),
            PipePullPlan::Continue
        );
        assert_eq!(
            active.plan_before_pull(pull_observation(ReadableState::Readable, true, true)),
            PipePullPlan::MarkReadPendingAndPull
        );
        assert_eq!(
            active.plan_before_pull(pull_observation(ReadableState::Readable, true, false)),
            PipePullPlan::BlockedByDestination
        );
        assert_eq!(
            active.plan_before_pull(pull_observation(ReadableState::Readable, false, false)),
            PipePullPlan::BlockedByDestination
        );
        assert_eq!(
            active.plan_before_pull(pull_observation(ReadableState::Closed, true, false)),
            PipePullPlan::Continue
        );
        assert_eq!(
            finished_state().plan_before_pull(pull_observation(
                ReadableState::Readable,
                true,
                true,
            )),
            PipePullPlan::Stop
        );
    }

    #[test]
    fn initial_terminal_plan_owns_endpoint_precedence() {
        let active = PipeOwnerState::new(DEFAULT_OPTIONS);
        let observation = |source_state, destination_state, close_requested| {
            PipeEndpointObservation::new(
                source_state,
                PipeDestinationObservation::new(destination_state, close_requested),
            )
        };
        assert_eq!(
            active.plan_initial_terminal(observation(
                ReadableState::Errored,
                WritableState::Errored,
                true,
            )),
            PipeInitialTerminalPlan::Claim(PipeEndpointTerminalTrigger::SourceErrored)
        );
        assert_eq!(
            active.plan_initial_terminal(observation(
                ReadableState::Closed,
                WritableState::Errored,
                false,
            )),
            PipeInitialTerminalPlan::Claim(PipeEndpointTerminalTrigger::DestinationErrored)
        );
        assert_eq!(
            active.plan_initial_terminal(observation(
                ReadableState::Closed,
                WritableState::Closed,
                false,
            )),
            PipeInitialTerminalPlan::Claim(PipeEndpointTerminalTrigger::SourceClosed)
        );
        assert_eq!(
            active.plan_initial_terminal(observation(
                ReadableState::Readable,
                WritableState::Writable,
                true,
            )),
            PipeInitialTerminalPlan::Claim(PipeEndpointTerminalTrigger::DestinationClosed)
        );
        assert_eq!(
            active.plan_initial_terminal(observation(
                ReadableState::Readable,
                WritableState::Writable,
                false,
            )),
            PipeInitialTerminalPlan::Continue
        );
        assert_eq!(
            finished_state().plan_initial_terminal(observation(
                ReadableState::Errored,
                WritableState::Errored,
                true,
            )),
            PipeInitialTerminalPlan::Stop
        );
    }

    #[test]
    fn shutdown_operation_plan_owns_endpoint_no_ops() {
        let aborting = PipeOwnerState::new(DEFAULT_OPTIONS)
            .transition_shutdown(PipeShutdownEvent::ClaimTerminal {
                trigger: PipeTerminalTrigger::Aborted,
                destination: IMMEDIATE_DESTINATION,
            })
            .next();
        assert_eq!(
            aborting.plan_shutdown_operation(PipeShutdownOperationObservation::AbortDestination {
                destination_state: WritableState::Errored,
            },),
            PipeShutdownOperationPlan::FulfillWithoutRunning
        );
        assert_eq!(
            aborting.plan_shutdown_operation(PipeShutdownOperationObservation::AbortDestination {
                destination_state: WritableState::Writable,
            },),
            PipeShutdownOperationPlan::Run
        );
        assert_eq!(
            aborting.plan_shutdown_operation(PipeShutdownOperationObservation::CancelSource {
                source_state: ReadableState::Closed,
            }),
            PipeShutdownOperationPlan::FulfillWithoutRunning
        );

        let source_error = PipeOwnerState::new(DEFAULT_OPTIONS)
            .transition_shutdown(PipeShutdownEvent::ClaimTerminal {
                trigger: PipeTerminalTrigger::SourceErrored,
                destination: IMMEDIATE_DESTINATION,
            })
            .next();
        assert_eq!(
            source_error.plan_shutdown_operation(
                PipeShutdownOperationObservation::AbortDestination {
                    destination_state: WritableState::Errored,
                },
            ),
            PipeShutdownOperationPlan::Run
        );
    }

    #[test]
    fn storage_decode_rejects_cross_lifecycle_and_settlement_states() {
        let empty = PipeShutdownSettlements::from_bits(0).expect("empty settlements");
        assert!(PipeShutdownSettlements::from_bits(1 << 2).is_none());

        let active = PipeOwnerState::new(DEFAULT_OPTIONS);
        assert_eq!(
            PipeOwnerState::from_storage(
                active.lifecycle,
                active.options,
                active.drain_scheduled,
                active.read_pending,
                active.write_in_progress,
                active.drain_yield_once,
                active.abort_listener,
                active.shutdown_settlements,
            ),
            Some(active)
        );
        assert!(
            PipeOwnerState::from_storage(
                PipeLifecycle::Finished,
                DEFAULT_OPTIONS,
                false,
                true,
                false,
                false,
                AbortListenerState::None,
                empty,
            )
            .is_none()
        );
        assert!(
            PipeOwnerState::from_storage(
                PipeLifecycle::ShuttingDown {
                    stage: PipeShutdownStage::Finalizing,
                    trigger: PipeTerminalTrigger::SourceClosed,
                },
                DEFAULT_OPTIONS,
                false,
                false,
                false,
                false,
                AbortListenerState::None,
                empty,
            )
            .is_none()
        );
    }

    #[test]
    fn source_destination_and_abort_propagation_honor_each_prevent_flag() {
        for prevent_close in [false, true] {
            for prevent_abort in [false, true] {
                for prevent_cancel in [false, true] {
                    let options = PipeOptions::new(prevent_close, prevent_abort, prevent_cancel);
                    let command_for = |trigger| {
                        PipeOwnerState::new(options)
                            .transition_shutdown(PipeShutdownEvent::ClaimTerminal {
                                trigger,
                                destination: IMMEDIATE_DESTINATION,
                            })
                            .command()
                    };

                    let source_error_action = if prevent_abort {
                        None
                    } else {
                        Some(PipeShutdownAction::AbortDestination)
                    };
                    let destination_action = if prevent_cancel {
                        None
                    } else {
                        Some(PipeShutdownAction::CancelSource)
                    };
                    let source_close_action = if prevent_close {
                        None
                    } else {
                        Some(PipeShutdownAction::CloseDestination)
                    };
                    let abort_action = match (prevent_abort, prevent_cancel) {
                        (false, false) => Some(PipeShutdownAction::AbortDestinationAndCancelSource),
                        (false, true) => Some(PipeShutdownAction::AbortDestination),
                        (true, false) => Some(PipeShutdownAction::CancelSource),
                        (true, true) => None,
                    };

                    for (trigger, expected_action) in [
                        (PipeTerminalTrigger::SourceErrored, source_error_action),
                        (PipeTerminalTrigger::DestinationErrored, destination_action),
                        (PipeTerminalTrigger::DestinationClosed, destination_action),
                        (PipeTerminalTrigger::SourceClosed, source_close_action),
                        (PipeTerminalTrigger::Aborted, abort_action),
                    ] {
                        let command = command_for(trigger);
                        if let Some(action) = expected_action {
                            assert_eq!(command, Some(PipeShutdownCommand::RunActions { action }));
                        } else {
                            assert!(matches!(
                                command,
                                Some(PipeShutdownCommand::Finalize { .. })
                            ));
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn abort_listener_state_distinguishes_registered_and_dispatching() {
        let active = PipeOwnerState::new(DEFAULT_OPTIONS);
        let registered = active
            .transition_mutation(PipeOwnerMutation::AbortListenerRegistered)
            .next();
        assert_eq!(registered.abort_listener(), AbortListenerState::Registered);

        let aborting = registered
            .transition_mutation(PipeOwnerMutation::AbortDispatchStarted)
            .next();
        assert_eq!(aborting.abort_listener(), AbortListenerState::Aborting);

        let shutting_down = aborting
            .transition_shutdown(PipeShutdownEvent::ClaimTerminal {
                trigger: PipeTerminalTrigger::Aborted,
                destination: IMMEDIATE_DESTINATION,
            })
            .next();
        assert_eq!(shutting_down.abort_listener(), AbortListenerState::Aborting);

        let destination_settled = shutting_down
            .transition_shutdown(PipeShutdownEvent::ActionFulfilled {
                operation: PipeShutdownOperation::Destination,
            })
            .next();
        let finalizing = destination_settled
            .transition_shutdown(PipeShutdownEvent::ActionFulfilled {
                operation: PipeShutdownOperation::Source,
            })
            .next();
        let finished = finalizing
            .transition_shutdown(PipeShutdownEvent::Finalize)
            .next();
        assert_eq!(finished.abort_listener(), AbortListenerState::None);
    }

    #[test]
    fn options_round_trip_the_compact_storage_mask() {
        for prevent_close in [false, true] {
            for prevent_abort in [false, true] {
                for prevent_cancel in [false, true] {
                    let options = PipeOptions::new(prevent_close, prevent_abort, prevent_cancel);
                    assert_eq!(PipeOptions::from_bits(options.bits()), Some(options));
                }
            }
        }
        assert_eq!(PipeOptions::from_bits(1 << 7), None);
    }

    #[test]
    fn entry_lock_plans_are_typed() {
        assert_eq!(
            PipeEntryObservation::new(false, false).plan(),
            PipeEntryPlan::Start
        );
        assert_eq!(
            PipeEntryObservation::new(true, false).plan(),
            PipeEntryPlan::RejectSourceLocked
        );
        assert_eq!(
            PipeEntryObservation::new(false, true).plan(),
            PipeEntryPlan::RejectDestinationLocked
        );
    }

    #[test]
    fn terminal_claim_makes_an_already_scheduled_drain_callback_stale() {
        let initial = PipeOwnerState::new(DEFAULT_OPTIONS);
        let scheduled = initial.transition_drain(PipeDrainEvent::Schedule);
        assert_eq!(scheduled.command(), Some(PipeDrainCommand::Enqueue));

        let claimed = scheduled
            .next()
            .transition_shutdown(PipeShutdownEvent::ClaimTerminal {
                trigger: PipeTerminalTrigger::SourceErrored,
                destination: IMMEDIATE_DESTINATION,
            });
        assert!(matches!(
            claimed.next().lifecycle(),
            PipeLifecycle::ShuttingDown { .. }
        ));
        assert_eq!(
            claimed.command(),
            Some(PipeShutdownCommand::RunActions {
                action: PipeShutdownAction::AbortDestination,
            })
        );

        let stale = claimed.next().transition_drain(PipeDrainEvent::Callback);
        assert_eq!(stale.admission(), PipeAdmission::Ignored);
        assert_eq!(stale.next(), claimed.next());
    }

    #[test]
    fn first_terminal_event_is_the_only_shutdown_owner() {
        let source_error = PipeOwnerState::new(DEFAULT_OPTIONS).transition_shutdown(
            PipeShutdownEvent::ClaimTerminal {
                trigger: PipeTerminalTrigger::SourceErrored,
                destination: IMMEDIATE_DESTINATION,
            },
        );
        let competing_destination_error =
            source_error
                .next()
                .transition_shutdown(PipeShutdownEvent::ClaimTerminal {
                    trigger: PipeTerminalTrigger::DestinationErrored,
                    destination: IMMEDIATE_DESTINATION,
                });
        assert_eq!(
            competing_destination_error.admission(),
            PipeAdmission::Ignored
        );
        assert_eq!(
            competing_destination_error.next().terminal_trigger(),
            Some(PipeTerminalTrigger::SourceErrored)
        );
    }

    #[test]
    fn source_close_waits_for_last_write_and_close_action_before_resolving() {
        let waiting = PipeOwnerState::new(DEFAULT_OPTIONS).transition_shutdown(
            PipeShutdownEvent::ClaimTerminal {
                trigger: PipeTerminalTrigger::SourceClosed,
                destination: WRITABLE_DESTINATION,
            },
        );
        assert_eq!(
            waiting.command(),
            Some(PipeShutdownCommand::WaitForLastWrite)
        );
        assert!(matches!(
            waiting.next().lifecycle(),
            PipeLifecycle::ShuttingDown {
                stage: PipeShutdownStage::WaitingForLastWrite,
                ..
            }
        ));

        let closing = waiting
            .next()
            .transition_shutdown(PipeShutdownEvent::LastWriteSettled);
        assert_eq!(
            closing.command(),
            Some(PipeShutdownCommand::RunActions {
                action: PipeShutdownAction::CloseDestination,
            })
        );

        let finished = closing
            .next()
            .transition_shutdown(PipeShutdownEvent::ActionFulfilled {
                operation: PipeShutdownOperation::Destination,
            });
        assert_eq!(
            finished.command(),
            Some(PipeShutdownCommand::EnqueueFinalize)
        );
        assert!(matches!(
            finished.next().lifecycle(),
            PipeLifecycle::ShuttingDown { .. }
        ));

        let finalized = finished
            .next()
            .transition_shutdown(PipeShutdownEvent::Finalize);
        assert!(matches!(
            finalized.command(),
            Some(PipeShutdownCommand::Finalize {
                finalization: PipeFinalization {
                    settlement: PipeFinalSettlement::Resolve,
                    ..
                }
            })
        ));
        assert_eq!(finalized.next().lifecycle(), PipeLifecycle::Finished);

        let stale_rejection =
            finalized
                .next()
                .transition_shutdown(PipeShutdownEvent::ActionRejected {
                    operation: PipeShutdownOperation::Source,
                });
        assert_eq!(stale_rejection.admission(), PipeAdmission::Ignored);

        let stale_drain = finalized.next().transition_drain(PipeDrainEvent::Callback);
        assert_eq!(stale_drain.admission(), PipeAdmission::Ignored);
        assert_eq!(stale_drain.next(), finalized.next());
    }

    #[test]
    fn reentrant_terminal_waits_for_write_promise_publication() {
        let writing =
            PipeOwnerState::new(DEFAULT_OPTIONS).transition_mutation(PipeOwnerMutation::BeginWrite);
        assert!(writing.next().write_in_progress());

        let claimed = writing
            .next()
            .transition_shutdown(PipeShutdownEvent::ClaimTerminal {
                trigger: PipeTerminalTrigger::SourceErrored,
                destination: WRITABLE_DESTINATION,
            });
        assert_eq!(claimed.command(), None);
        assert!(matches!(
            claimed.next().lifecycle(),
            PipeLifecycle::ShuttingDown {
                stage: PipeShutdownStage::WaitingForWritePublication,
                ..
            }
        ));
        assert!(claimed.next().write_in_progress());

        let published = claimed.next().publish_last_write();
        assert_eq!(
            published.command(),
            Some(PipeWritePublicationCommand::StoreLastWriteAndWait)
        );
        assert!(matches!(
            published.next().lifecycle(),
            PipeLifecycle::ShuttingDown {
                stage: PipeShutdownStage::WaitingForLastWrite,
                ..
            }
        ));
        assert!(!published.next().write_in_progress());

        let settled = published
            .next()
            .transition_shutdown(PipeShutdownEvent::LastWriteSettled);
        assert_eq!(
            settled.command(),
            Some(PipeShutdownCommand::RunActions {
                action: PipeShutdownAction::AbortDestination,
            })
        );
    }

    #[test]
    fn abort_joins_both_actions_and_destination_rejection_has_input_order_priority() {
        let aborting = PipeOwnerState::new(DEFAULT_OPTIONS).transition_shutdown(
            PipeShutdownEvent::ClaimTerminal {
                trigger: PipeTerminalTrigger::Aborted,
                destination: IMMEDIATE_DESTINATION,
            },
        );
        assert_eq!(
            aborting.command(),
            Some(PipeShutdownCommand::RunActions {
                action: PipeShutdownAction::AbortDestinationAndCancelSource,
            })
        );
        assert_eq!(
            aborting
                .next()
                .remaining_shutdown_action_mask()
                .count_ones(),
            2
        );

        let source_rejected =
            aborting
                .next()
                .transition_shutdown(PipeShutdownEvent::ActionRejected {
                    operation: PipeShutdownOperation::Source,
                });
        assert_eq!(source_rejected.command(), None);
        assert_eq!(
            source_rejected
                .next()
                .remaining_shutdown_action_mask()
                .count_ones(),
            1
        );

        let rejected =
            source_rejected
                .next()
                .transition_shutdown(PipeShutdownEvent::ActionRejected {
                    operation: PipeShutdownOperation::Destination,
                });
        assert!(matches!(
            rejected.command(),
            Some(PipeShutdownCommand::Finalize {
                finalization: PipeFinalization {
                    settlement: PipeFinalSettlement::RejectShutdownAction {
                        operation: PipeShutdownOperation::Destination,
                    },
                    ..
                }
            })
        ));
        assert_eq!(rejected.next().lifecycle(), PipeLifecycle::Finished);
    }

    #[test]
    fn duplicate_shutdown_settlement_is_rejected_by_operation_ownership() {
        let aborting = PipeOwnerState::new(DEFAULT_OPTIONS).transition_shutdown(
            PipeShutdownEvent::ClaimTerminal {
                trigger: PipeTerminalTrigger::Aborted,
                destination: IMMEDIATE_DESTINATION,
            },
        );
        let source_settled =
            aborting
                .next()
                .transition_shutdown(PipeShutdownEvent::ActionFulfilled {
                    operation: PipeShutdownOperation::Source,
                });
        assert_eq!(source_settled.command(), None);
        assert_eq!(
            source_settled
                .next()
                .remaining_shutdown_action_mask()
                .count_ones(),
            1
        );

        let duplicate =
            source_settled
                .next()
                .transition_shutdown(PipeShutdownEvent::ActionRejected {
                    operation: PipeShutdownOperation::Source,
                });
        assert_eq!(duplicate.admission(), PipeAdmission::Ignored);
        assert_eq!(duplicate.next(), source_settled.next());
    }

    #[test]
    fn finalize_callback_is_owned_only_by_the_finalizing_stage() {
        let active = PipeOwnerState::new(DEFAULT_OPTIONS);
        assert_eq!(
            active
                .transition_shutdown(PipeShutdownEvent::Finalize)
                .admission(),
            PipeAdmission::Ignored
        );

        let shutting_down = active.transition_shutdown(PipeShutdownEvent::ClaimTerminal {
            trigger: PipeTerminalTrigger::SourceClosed,
            destination: WRITABLE_DESTINATION,
        });
        assert_eq!(
            shutting_down
                .next()
                .transition_shutdown(PipeShutdownEvent::Finalize)
                .admission(),
            PipeAdmission::Ignored
        );

        let running = shutting_down
            .next()
            .transition_shutdown(PipeShutdownEvent::LastWriteSettled);
        assert_eq!(
            running
                .next()
                .transition_shutdown(PipeShutdownEvent::Finalize)
                .admission(),
            PipeAdmission::Ignored
        );

        let finalizing = running
            .next()
            .transition_shutdown(PipeShutdownEvent::ActionFulfilled {
                operation: PipeShutdownOperation::Destination,
            });
        assert_eq!(
            finalizing.command(),
            Some(PipeShutdownCommand::EnqueueFinalize)
        );
        assert!(matches!(
            finalizing.next().lifecycle(),
            PipeLifecycle::ShuttingDown {
                stage: PipeShutdownStage::Finalizing,
                ..
            }
        ));
        assert!(matches!(
            finalizing
                .next()
                .transition_shutdown(PipeShutdownEvent::Finalize)
                .command(),
            Some(PipeShutdownCommand::Finalize { .. })
        ));
    }

    #[test]
    fn prevent_flags_can_finalize_without_running_a_shutdown_action() {
        let options = PipeOptions::new(true, true, true);
        for (trigger, settlement) in [
            (
                PipeTerminalTrigger::SourceErrored,
                PipeFinalSettlement::RejectOriginal,
            ),
            (
                PipeTerminalTrigger::DestinationErrored,
                PipeFinalSettlement::RejectOriginal,
            ),
            (
                PipeTerminalTrigger::SourceClosed,
                PipeFinalSettlement::Resolve,
            ),
            (
                PipeTerminalTrigger::DestinationClosed,
                PipeFinalSettlement::RejectOriginal,
            ),
            (
                PipeTerminalTrigger::Aborted,
                PipeFinalSettlement::RejectOriginal,
            ),
        ] {
            let transition = PipeOwnerState::new(options).transition_shutdown(
                PipeShutdownEvent::ClaimTerminal {
                    trigger,
                    destination: IMMEDIATE_DESTINATION,
                },
            );
            assert!(matches!(
                transition.command(),
                Some(PipeShutdownCommand::Finalize {
                    finalization: PipeFinalization {
                        settlement: actual,
                        ..
                    }
                }) if actual == settlement
            ));
            assert_eq!(transition.next().lifecycle(), PipeLifecycle::Finished);
        }
    }
}
