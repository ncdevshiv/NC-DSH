//! Pull scheduling state for `ReadableStreamDefaultController` and byte
//! controllers sharing the same underlying-source pull contract.

use super::ReadableState;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PullState {
    started: bool,
    pulling: bool,
    pull_again: bool,
}

impl PullState {
    #[must_use]
    pub const fn new(started: bool, pulling: bool, pull_again: bool) -> Self {
        Self {
            started,
            pulling,
            pull_again,
        }
    }

    #[must_use]
    pub const fn started(self) -> bool {
        self.started
    }

    #[must_use]
    pub const fn pulling(self) -> bool {
        self.pulling
    }

    #[must_use]
    pub const fn pull_again(self) -> bool {
        self.pull_again
    }

    #[must_use]
    pub const fn mark_started(self) -> Self {
        Self {
            started: true,
            ..self
        }
    }

    #[must_use]
    pub const fn begin_pull(self) -> Self {
        Self {
            pulling: true,
            pull_again: false,
            ..self
        }
    }

    #[must_use]
    pub const fn request_pull_again(self) -> Self {
        Self {
            pull_again: true,
            ..self
        }
    }

    #[must_use]
    pub const fn pull_fulfilled(self) -> PullSettlementPlan {
        let action = if self.pull_again {
            PullSettlementAction::PullAgain
        } else {
            PullSettlementAction::Idle
        };
        PullSettlementPlan {
            source: self,
            next: Self {
                pulling: false,
                pull_again: false,
                ..self
            },
            action,
        }
    }

    #[must_use]
    pub const fn pull_rejected(self) -> PullSettlementPlan {
        PullSettlementPlan {
            source: self,
            next: Self {
                pulling: false,
                ..self
            },
            action: PullSettlementAction::Error,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DefaultControllerSnapshot {
    stream_state: ReadableState,
    close_requested: bool,
    queue_available: bool,
    pull: PullState,
}

impl DefaultControllerSnapshot {
    #[must_use]
    pub const fn new(
        stream_state: ReadableState,
        close_requested: bool,
        queue_available: bool,
        pull: PullState,
    ) -> Self {
        Self {
            stream_state,
            close_requested,
            queue_available,
            pull,
        }
    }

    #[must_use]
    pub const fn pull_state(self) -> PullState {
        self.pull
    }

    #[must_use]
    pub const fn can_consider_pull(self) -> bool {
        self.stream_state.is_readable()
            && !self.close_requested
            && self.pull.started
            && self.queue_available
    }

    #[must_use]
    pub const fn plan_pull(self, has_demand: bool) -> PullPlan {
        if !self.can_consider_pull() || !has_demand {
            return PullPlan::None;
        }
        if self.pull.pulling {
            return PullPlan::MarkPullAgain(PullStatePlan {
                source: self.pull,
                next: self.pull.request_pull_again(),
            });
        }
        PullPlan::Start(PullStatePlan {
            source: self.pull,
            next: self.pull.begin_pull(),
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PullPlan {
    None,
    MarkPullAgain(PullStatePlan),
    Start(PullStatePlan),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PullStatePlan {
    source: PullState,
    next: PullState,
}

impl PullStatePlan {
    #[must_use]
    pub const fn source(self) -> PullState {
        self.source
    }

    #[must_use]
    pub const fn next(self) -> PullState {
        self.next
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PullSettlementPlan {
    source: PullState,
    next: PullState,
    action: PullSettlementAction,
}

impl PullSettlementPlan {
    #[must_use]
    pub const fn source(self) -> PullState {
        self.source
    }

    #[must_use]
    pub const fn next(self) -> PullState {
        self.next
    }

    #[must_use]
    pub const fn action(self) -> PullSettlementAction {
        self.action
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PullSettlementAction {
    Idle,
    PullAgain,
    Error,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(
        stream_state: ReadableState,
        close_requested: bool,
        queue_available: bool,
        pull: PullState,
    ) -> DefaultControllerSnapshot {
        DefaultControllerSnapshot::new(stream_state, close_requested, queue_available, pull)
    }

    #[test]
    fn pull_preconditions_cover_lifecycle_start_and_queue_storage() {
        let idle = PullState::new(true, false, false);
        assert!(!snapshot(ReadableState::Closed, false, true, idle).can_consider_pull());
        assert!(!snapshot(ReadableState::Errored, false, true, idle).can_consider_pull());
        assert!(!snapshot(ReadableState::Readable, true, true, idle).can_consider_pull());
        assert!(!snapshot(ReadableState::Readable, false, false, idle).can_consider_pull());
        assert!(
            !snapshot(
                ReadableState::Readable,
                false,
                true,
                PullState::new(false, false, false),
            )
            .can_consider_pull()
        );
        assert!(snapshot(ReadableState::Readable, false, true, idle).can_consider_pull());
    }

    #[test]
    fn demand_starts_one_pull_or_records_pull_again() {
        let idle = PullState::new(true, false, false);
        let ready = snapshot(ReadableState::Readable, false, true, idle);
        assert_eq!(ready.plan_pull(false), PullPlan::None);
        assert_eq!(
            ready.plan_pull(true),
            PullPlan::Start(PullStatePlan {
                source: idle,
                next: PullState::new(true, true, false),
            })
        );

        let pulling = PullState::new(true, true, false);
        assert_eq!(
            snapshot(ReadableState::Readable, false, true, pulling).plan_pull(true),
            PullPlan::MarkPullAgain(PullStatePlan {
                source: pulling,
                next: PullState::new(true, true, true),
            })
        );
    }

    #[test]
    fn pull_settlement_clears_the_in_flight_state_once() {
        let pulling = PullState::new(true, true, false);
        let fulfilled = pulling.pull_fulfilled();
        assert_eq!(fulfilled.source(), pulling);
        assert_eq!(fulfilled.next(), PullState::new(true, false, false));
        assert_eq!(fulfilled.action(), PullSettlementAction::Idle);

        let again = PullState::new(true, true, true).pull_fulfilled();
        assert_eq!(again.next(), PullState::new(true, false, false));
        assert_eq!(again.action(), PullSettlementAction::PullAgain);

        let rejected = PullState::new(true, true, true).pull_rejected();
        assert_eq!(rejected.next(), PullState::new(true, false, true));
        assert_eq!(rejected.action(), PullSettlementAction::Error);
    }

    #[test]
    fn marking_started_preserves_other_pull_flags() {
        assert_eq!(
            PullState::new(false, true, true).mark_started(),
            PullState::new(true, true, true)
        );
    }
}
