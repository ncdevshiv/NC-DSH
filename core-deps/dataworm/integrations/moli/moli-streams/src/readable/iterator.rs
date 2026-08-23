//! Runtime-independent lifecycle for a `ReadableStream` async iterator.
//!
//! JavaScript values, Promise resolvers, and queued operation identities stay
//! in the renderer. This module owns the serial-operation invariant and the
//! lifecycle transition selected for each queue-head event.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IteratorLifecycle {
    Active,
    Returning,
    Closed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IteratorOperationKind {
    Next,
    Return,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IteratorState {
    lifecycle: IteratorLifecycle,
    operation_active: bool,
}

impl IteratorState {
    #[must_use]
    pub const fn new(lifecycle: IteratorLifecycle, operation_active: bool) -> Self {
        Self {
            lifecycle,
            operation_active,
        }
    }

    #[must_use]
    pub const fn lifecycle(self) -> IteratorLifecycle {
        self.lifecycle
    }

    #[must_use]
    pub const fn operation_active(self) -> bool {
        self.operation_active
    }

    #[must_use]
    pub const fn plan_pump(self, head: Option<IteratorOperationKind>) -> IteratorPumpPlan {
        if self.operation_active || matches!(self.lifecycle, IteratorLifecycle::Returning) {
            return IteratorPumpPlan::WaitForInFlight;
        }
        let Some(head) = head else {
            return IteratorPumpPlan::Idle;
        };
        match (self.lifecycle, head) {
            (IteratorLifecycle::Active, IteratorOperationKind::Next) => {
                IteratorPumpPlan::StartNext(self.transition(IteratorLifecycle::Active, true))
            }
            (IteratorLifecycle::Active, IteratorOperationKind::Return) => {
                IteratorPumpPlan::StartReturn(self.transition(IteratorLifecycle::Returning, true))
            }
            (IteratorLifecycle::Closed, IteratorOperationKind::Next) => {
                IteratorPumpPlan::ResolveClosedNext
            }
            (IteratorLifecycle::Closed, IteratorOperationKind::Return) => {
                IteratorPumpPlan::ResolveClosedReturn
            }
            (IteratorLifecycle::Returning, _) => IteratorPumpPlan::WaitForInFlight,
        }
    }

    #[must_use]
    pub const fn plan_next_settlement(self, outcome: IteratorNextOutcome) -> IteratorTransition {
        assert!(
            self.operation_active && matches!(self.lifecycle, IteratorLifecycle::Active),
            "next settlement requires an active next operation"
        );
        let lifecycle = match outcome {
            IteratorNextOutcome::Chunk => IteratorLifecycle::Active,
            IteratorNextOutcome::Done | IteratorNextOutcome::Rejected => IteratorLifecycle::Closed,
        };
        self.transition(lifecycle, false)
    }

    #[must_use]
    pub const fn plan_return_settlement(self) -> IteratorTransition {
        assert!(
            self.operation_active && matches!(self.lifecycle, IteratorLifecycle::Returning),
            "return settlement requires an active return operation"
        );
        self.transition(IteratorLifecycle::Closed, false)
    }

    const fn transition(
        self,
        lifecycle: IteratorLifecycle,
        operation_active: bool,
    ) -> IteratorTransition {
        IteratorTransition {
            source: self,
            next: Self::new(lifecycle, operation_active),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IteratorPumpPlan {
    Idle,
    WaitForInFlight,
    StartNext(IteratorTransition),
    StartReturn(IteratorTransition),
    ResolveClosedNext,
    ResolveClosedReturn,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IteratorNextOutcome {
    Chunk,
    Done,
    Rejected,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IteratorTransition {
    source: IteratorState,
    next: IteratorState,
}

impl IteratorTransition {
    #[must_use]
    pub const fn source(self) -> IteratorState {
        self.source
    }

    #[must_use]
    pub const fn next(self) -> IteratorState {
        self.next
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ACTIVE: IteratorState = IteratorState::new(IteratorLifecycle::Active, false);
    const CLOSED: IteratorState = IteratorState::new(IteratorLifecycle::Closed, false);

    #[test]
    fn pump_serializes_operations_and_preserves_closed_return_values() {
        let IteratorPumpPlan::StartNext(next) = ACTIVE.plan_pump(Some(IteratorOperationKind::Next))
        else {
            panic!("active next must start");
        };
        assert_eq!(
            next.next().plan_pump(Some(IteratorOperationKind::Return)),
            IteratorPumpPlan::WaitForInFlight
        );

        let IteratorPumpPlan::StartReturn(returning) =
            ACTIVE.plan_pump(Some(IteratorOperationKind::Return))
        else {
            panic!("active return must start");
        };
        assert_eq!(returning.next().lifecycle(), IteratorLifecycle::Returning);
        assert_eq!(
            returning
                .next()
                .plan_pump(Some(IteratorOperationKind::Next)),
            IteratorPumpPlan::WaitForInFlight
        );
        assert_eq!(
            CLOSED.plan_pump(Some(IteratorOperationKind::Next)),
            IteratorPumpPlan::ResolveClosedNext
        );
        assert_eq!(
            CLOSED.plan_pump(Some(IteratorOperationKind::Return)),
            IteratorPumpPlan::ResolveClosedReturn
        );
    }

    #[test]
    fn next_and_return_settlements_release_the_single_in_flight_owner() {
        let IteratorPumpPlan::StartNext(next) = ACTIVE.plan_pump(Some(IteratorOperationKind::Next))
        else {
            panic!("active next must start");
        };
        let next = next.next();
        assert_eq!(
            next.plan_next_settlement(IteratorNextOutcome::Chunk).next(),
            ACTIVE
        );
        assert_eq!(
            next.plan_next_settlement(IteratorNextOutcome::Done).next(),
            CLOSED
        );
        assert_eq!(
            next.plan_next_settlement(IteratorNextOutcome::Rejected)
                .next(),
            CLOSED
        );

        let IteratorPumpPlan::StartReturn(returning) =
            ACTIVE.plan_pump(Some(IteratorOperationKind::Return))
        else {
            panic!("active return must start");
        };
        assert_eq!(returning.next().plan_return_settlement().next(), CLOSED);
    }
}
