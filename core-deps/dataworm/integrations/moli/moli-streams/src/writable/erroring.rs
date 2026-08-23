//! Runtime-independent planning for the two-phase writable error lifecycle.
//!
//! A writable stream first enters `erroring`, retaining its original error
//! while `start()` or an already in-flight write/close owns settlement.  Only
//! after those barriers clear may it become `errored`, reject queued requests,
//! and settle a pending abort request.  Promise objects and JavaScript reasons
//! remain adapter-owned.

use super::WritableState;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PendingAbortState {
    None,
    InitiatedErroring,
    AlreadyErroring,
}

impl PendingAbortState {
    #[must_use]
    pub const fn is_pending(self) -> bool {
        !matches!(self, Self::None)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AbortPlan {
    Resolve,
    ReusePending,
    CreatePending {
        was_already_erroring: bool,
        start_erroring: bool,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ErrorPlan {
    Ignore,
    Start { finish_immediately: bool },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FinishErroringPlan {
    Ignore,
    Wait,
    FinishWithoutAbort,
    FinishAndRejectAbort,
    FinishAndRunAbort,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CloseOutcome {
    Fulfilled,
    Rejected,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CloseSettlementPlan {
    Close {
        clear_stored_error: bool,
        resolve_pending_abort: bool,
    },
    Reject {
        reject_pending_abort: bool,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ErroringSnapshot {
    state: WritableState,
    start_pending: bool,
    operation_in_flight: bool,
    pending_abort: PendingAbortState,
}

impl ErroringSnapshot {
    #[must_use]
    pub const fn new(
        state: WritableState,
        start_pending: bool,
        operation_in_flight: bool,
        pending_abort: PendingAbortState,
    ) -> Self {
        Self {
            state,
            start_pending,
            operation_in_flight,
            pending_abort,
        }
    }

    #[must_use]
    pub const fn plan_abort(self) -> AbortPlan {
        if matches!(self.state, WritableState::Closed | WritableState::Errored) {
            return AbortPlan::Resolve;
        }
        if self.pending_abort.is_pending() {
            return AbortPlan::ReusePending;
        }
        match self.state {
            WritableState::Writable => AbortPlan::CreatePending {
                was_already_erroring: false,
                start_erroring: true,
            },
            WritableState::Erroring => AbortPlan::CreatePending {
                was_already_erroring: true,
                start_erroring: false,
            },
            WritableState::Closed | WritableState::Errored => unreachable!(),
        }
    }

    #[must_use]
    pub const fn plan_error(self) -> ErrorPlan {
        if matches!(self.state, WritableState::Writable) {
            ErrorPlan::Start {
                finish_immediately: !self.start_pending && !self.operation_in_flight,
            }
        } else {
            ErrorPlan::Ignore
        }
    }

    #[must_use]
    pub const fn plan_finish(self) -> FinishErroringPlan {
        if !matches!(self.state, WritableState::Erroring) {
            return FinishErroringPlan::Ignore;
        }
        if self.start_pending || self.operation_in_flight {
            return FinishErroringPlan::Wait;
        }
        match self.pending_abort {
            PendingAbortState::None => FinishErroringPlan::FinishWithoutAbort,
            PendingAbortState::InitiatedErroring => FinishErroringPlan::FinishAndRunAbort,
            PendingAbortState::AlreadyErroring => FinishErroringPlan::FinishAndRejectAbort,
        }
    }

    #[must_use]
    pub const fn plan_close_settlement(self, outcome: CloseOutcome) -> CloseSettlementPlan {
        debug_assert!(matches!(
            self.state,
            WritableState::Writable | WritableState::Erroring
        ));
        match outcome {
            CloseOutcome::Fulfilled => CloseSettlementPlan::Close {
                clear_stored_error: matches!(self.state, WritableState::Erroring),
                resolve_pending_abort: self.pending_abort.is_pending(),
            },
            CloseOutcome::Rejected => CloseSettlementPlan::Reject {
                reject_pending_abort: self.pending_abort.is_pending(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(
        state: WritableState,
        start_pending: bool,
        operation_in_flight: bool,
        pending_abort: PendingAbortState,
    ) -> ErroringSnapshot {
        ErroringSnapshot::new(state, start_pending, operation_in_flight, pending_abort)
    }

    #[test]
    fn abort_claims_exactly_one_residence() {
        assert_eq!(
            snapshot(
                WritableState::Writable,
                false,
                false,
                PendingAbortState::None,
            )
            .plan_abort(),
            AbortPlan::CreatePending {
                was_already_erroring: false,
                start_erroring: true,
            }
        );
        assert_eq!(
            snapshot(
                WritableState::Erroring,
                true,
                false,
                PendingAbortState::None,
            )
            .plan_abort(),
            AbortPlan::CreatePending {
                was_already_erroring: true,
                start_erroring: false,
            }
        );
        assert_eq!(
            snapshot(
                WritableState::Erroring,
                true,
                false,
                PendingAbortState::AlreadyErroring,
            )
            .plan_abort(),
            AbortPlan::ReusePending
        );
        assert_eq!(
            snapshot(
                WritableState::Errored,
                false,
                false,
                PendingAbortState::None,
            )
            .plan_abort(),
            AbortPlan::Resolve
        );
    }

    #[test]
    fn start_and_in_flight_operations_are_finish_barriers() {
        for (start_pending, operation_in_flight) in [(true, false), (false, true), (true, true)] {
            assert_eq!(
                snapshot(
                    WritableState::Erroring,
                    start_pending,
                    operation_in_flight,
                    PendingAbortState::AlreadyErroring,
                )
                .plan_finish(),
                FinishErroringPlan::Wait
            );
        }
        assert_eq!(
            snapshot(
                WritableState::Erroring,
                false,
                false,
                PendingAbortState::AlreadyErroring,
            )
            .plan_finish(),
            FinishErroringPlan::FinishAndRejectAbort
        );
        assert_eq!(
            snapshot(
                WritableState::Erroring,
                false,
                false,
                PendingAbortState::InitiatedErroring,
            )
            .plan_finish(),
            FinishErroringPlan::FinishAndRunAbort
        );
    }

    #[test]
    fn in_flight_close_owns_pending_abort_settlement() {
        assert_eq!(
            snapshot(
                WritableState::Erroring,
                false,
                false,
                PendingAbortState::InitiatedErroring,
            )
            .plan_close_settlement(CloseOutcome::Fulfilled),
            CloseSettlementPlan::Close {
                clear_stored_error: true,
                resolve_pending_abort: true,
            }
        );
        assert_eq!(
            snapshot(
                WritableState::Erroring,
                false,
                false,
                PendingAbortState::InitiatedErroring,
            )
            .plan_close_settlement(CloseOutcome::Rejected),
            CloseSettlementPlan::Reject {
                reject_pending_abort: true,
            }
        );
        assert_eq!(
            snapshot(
                WritableState::Writable,
                false,
                false,
                PendingAbortState::None,
            )
            .plan_close_settlement(CloseOutcome::Fulfilled),
            CloseSettlementPlan::Close {
                clear_stored_error: false,
                resolve_pending_abort: false,
            }
        );
    }
}
