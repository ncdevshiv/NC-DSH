//! Runtime-independent ownership plans for a `WritableStreamDefaultWriter`.
//!
//! The JavaScript promises remain renderer-owned. This module describes the
//! stable residence each writer must own and how lifecycle transitions update
//! that residence without manufacturing a fresh promise from a getter.

use super::WritableState;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PromiseResidenceState {
    Missing,
    Pending,
    Fulfilled,
    Rejected,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InitialPromiseState {
    Pending,
    Fulfilled,
    RejectedStoredError,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WriterPromiseInitialization {
    ready: InitialPromiseState,
    closed: InitialPromiseState,
}

impl WriterPromiseInitialization {
    #[must_use]
    pub const fn ready(self) -> InitialPromiseState {
        self.ready
    }

    #[must_use]
    pub const fn closed(self) -> InitialPromiseState {
        self.closed
    }
}

/// Plans the two promise residences created when a writer acquires a stream.
/// A queued close makes `ready` fulfilled even if the last stored queue total
/// still projects backpressure.
#[must_use]
pub const fn plan_writer_promise_initialization(
    state: WritableState,
    close_queued_or_in_flight: bool,
    backpressure: bool,
) -> WriterPromiseInitialization {
    match state {
        WritableState::Writable => WriterPromiseInitialization {
            ready: if close_queued_or_in_flight || !backpressure {
                InitialPromiseState::Fulfilled
            } else {
                InitialPromiseState::Pending
            },
            closed: InitialPromiseState::Pending,
        },
        WritableState::Erroring => WriterPromiseInitialization {
            ready: InitialPromiseState::RejectedStoredError,
            closed: InitialPromiseState::Pending,
        },
        WritableState::Closed => WriterPromiseInitialization {
            ready: InitialPromiseState::Fulfilled,
            closed: InitialPromiseState::Fulfilled,
        },
        WritableState::Errored => WriterPromiseInitialization {
            ready: InitialPromiseState::RejectedStoredError,
            closed: InitialPromiseState::RejectedStoredError,
        },
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EnsurePendingPlan {
    Keep,
    ReplaceWithPending,
}

#[must_use]
pub const fn plan_ensure_pending(state: PromiseResidenceState) -> EnsurePendingPlan {
    match state {
        PromiseResidenceState::Pending => EnsurePendingPlan::Keep,
        PromiseResidenceState::Missing
        | PromiseResidenceState::Fulfilled
        | PromiseResidenceState::Rejected => EnsurePendingPlan::ReplaceWithPending,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResolvePromisePlan {
    Keep,
    ResolveCurrent,
}

#[must_use]
pub const fn plan_resolve(state: PromiseResidenceState) -> ResolvePromisePlan {
    match state {
        PromiseResidenceState::Pending => ResolvePromisePlan::ResolveCurrent,
        PromiseResidenceState::Missing
        | PromiseResidenceState::Fulfilled
        | PromiseResidenceState::Rejected => ResolvePromisePlan::Keep,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EnsureRejectedPlan {
    RejectCurrent,
    ReplaceAndReject,
}

/// Implements the Streams "ensure promise rejected" operation. A pending
/// residence is rejected in place; a settled or missing residence is replaced
/// so later getters observe one stable rejected promise.
#[must_use]
pub const fn plan_ensure_rejected(state: PromiseResidenceState) -> EnsureRejectedPlan {
    match state {
        PromiseResidenceState::Pending => EnsureRejectedPlan::RejectCurrent,
        PromiseResidenceState::Missing
        | PromiseResidenceState::Fulfilled
        | PromiseResidenceState::Rejected => EnsureRejectedPlan::ReplaceAndReject,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acquisition_initializes_both_writer_owned_residences() {
        assert_eq!(
            plan_writer_promise_initialization(WritableState::Writable, false, false),
            WriterPromiseInitialization {
                ready: InitialPromiseState::Fulfilled,
                closed: InitialPromiseState::Pending,
            }
        );
        assert_eq!(
            plan_writer_promise_initialization(WritableState::Writable, false, true),
            WriterPromiseInitialization {
                ready: InitialPromiseState::Pending,
                closed: InitialPromiseState::Pending,
            }
        );
        assert_eq!(
            plan_writer_promise_initialization(WritableState::Writable, true, true),
            WriterPromiseInitialization {
                ready: InitialPromiseState::Fulfilled,
                closed: InitialPromiseState::Pending,
            }
        );
        assert_eq!(
            plan_writer_promise_initialization(WritableState::Erroring, false, false),
            WriterPromiseInitialization {
                ready: InitialPromiseState::RejectedStoredError,
                closed: InitialPromiseState::Pending,
            }
        );
        assert_eq!(
            plan_writer_promise_initialization(WritableState::Closed, false, false),
            WriterPromiseInitialization {
                ready: InitialPromiseState::Fulfilled,
                closed: InitialPromiseState::Fulfilled,
            }
        );
        assert_eq!(
            plan_writer_promise_initialization(WritableState::Errored, false, false),
            WriterPromiseInitialization {
                ready: InitialPromiseState::RejectedStoredError,
                closed: InitialPromiseState::RejectedStoredError,
            }
        );
    }

    #[test]
    fn backpressure_replaces_only_a_settled_ready_residence() {
        assert_eq!(
            plan_ensure_pending(PromiseResidenceState::Fulfilled),
            EnsurePendingPlan::ReplaceWithPending
        );
        assert_eq!(
            plan_ensure_pending(PromiseResidenceState::Pending),
            EnsurePendingPlan::Keep
        );
        assert_eq!(
            plan_resolve(PromiseResidenceState::Pending),
            ResolvePromisePlan::ResolveCurrent
        );
        assert_eq!(
            plan_resolve(PromiseResidenceState::Fulfilled),
            ResolvePromisePlan::Keep
        );
    }

    #[test]
    fn error_and_release_reject_pending_in_place_but_replace_settled() {
        assert_eq!(
            plan_ensure_rejected(PromiseResidenceState::Pending),
            EnsureRejectedPlan::RejectCurrent
        );
        for state in [
            PromiseResidenceState::Missing,
            PromiseResidenceState::Fulfilled,
            PromiseResidenceState::Rejected,
        ] {
            assert_eq!(
                plan_ensure_rejected(state),
                EnsureRejectedPlan::ReplaceAndReject
            );
        }
    }
}
