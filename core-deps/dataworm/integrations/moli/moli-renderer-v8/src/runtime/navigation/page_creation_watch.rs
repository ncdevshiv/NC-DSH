//! One-shot navigation failure reporting for one Page creation scope.
//!
//! A pending creation owns the strong observer while the stable Page slot
//! keeps only a weak failure publisher. Once creation completes or is dropped,
//! later navigation failures cannot leak into an unrelated wait.

use std::cell::Cell;
use std::rc::{Rc, Weak};

use super::terminal::PageNavigationOwnerFailure;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::runtime) enum PageCreationNavigationFailurePublication {
    Recorded,
    AlreadyRecorded,
    NoActiveCreationObserver,
}

#[derive(Debug)]
pub(in crate::runtime) struct PageCreationNavigationFailurePublisher {
    // This scope never leaves its owner-local thread. `Cell` provides the
    // one-shot mutation without suggesting cross-thread synchronization.
    state: Weak<Cell<Option<PageNavigationOwnerFailure>>>,
}

impl PageCreationNavigationFailurePublisher {
    pub(in crate::runtime) fn publish(
        &self,
        failure: PageNavigationOwnerFailure,
    ) -> PageCreationNavigationFailurePublication {
        let Some(state) = self.state.upgrade() else {
            return PageCreationNavigationFailurePublication::NoActiveCreationObserver;
        };
        if state.get().is_some() {
            return PageCreationNavigationFailurePublication::AlreadyRecorded;
        }
        state.set(Some(failure));
        PageCreationNavigationFailurePublication::Recorded
    }
}

#[derive(Debug)]
pub(in crate::runtime) struct PageCreationNavigationFailureObserver {
    state: Rc<Cell<Option<PageNavigationOwnerFailure>>>,
}

impl PageCreationNavigationFailureObserver {
    pub(in crate::runtime) fn failure(&self) -> Option<PageNavigationOwnerFailure> {
        self.state.get()
    }
}

pub(in crate::runtime) fn page_creation_navigation_failure_scope() -> (
    PageCreationNavigationFailurePublisher,
    PageCreationNavigationFailureObserver,
) {
    let state = Rc::new(Cell::new(None));
    (
        PageCreationNavigationFailurePublisher {
            state: Rc::downgrade(&state),
        },
        PageCreationNavigationFailureObserver { state },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIRST_FAILURE: PageNavigationOwnerFailure =
        PageNavigationOwnerFailure::TooManyChainedLocationNavigations {
            context: "creating the test Page",
        };
    const SECOND_FAILURE: PageNavigationOwnerFailure =
        PageNavigationOwnerFailure::TooManyChainedLocationNavigations {
            context: "running unrelated later navigation",
        };

    #[test]
    fn active_creation_observer_sees_its_concrete_navigation_failure() {
        let (publisher, observer) = page_creation_navigation_failure_scope();

        assert_eq!(
            publisher.publish(FIRST_FAILURE),
            PageCreationNavigationFailurePublication::Recorded
        );
        assert_eq!(observer.failure(), Some(FIRST_FAILURE));
    }

    #[test]
    fn recorded_failure_survives_stable_page_publisher_teardown() {
        let (publisher, observer) = page_creation_navigation_failure_scope();

        assert_eq!(
            publisher.publish(FIRST_FAILURE),
            PageCreationNavigationFailurePublication::Recorded
        );
        drop(publisher);

        assert_eq!(
            observer.failure(),
            Some(FIRST_FAILURE),
            "a parked PageCreation turn must retain its terminal after the stable Page slot retires"
        );
    }

    #[test]
    fn first_terminal_remains_authoritative_within_one_creation_scope() {
        let (publisher, observer) = page_creation_navigation_failure_scope();

        assert_eq!(
            publisher.publish(FIRST_FAILURE),
            PageCreationNavigationFailurePublication::Recorded
        );
        assert_eq!(
            publisher.publish(SECOND_FAILURE),
            PageCreationNavigationFailurePublication::AlreadyRecorded
        );
        assert_eq!(observer.failure(), Some(FIRST_FAILURE));
    }

    #[test]
    fn completed_creation_cannot_leak_failure_into_a_later_observer() {
        let (publisher, observer) = page_creation_navigation_failure_scope();
        drop(observer);

        assert_eq!(
            publisher.publish(FIRST_FAILURE),
            PageCreationNavigationFailurePublication::NoActiveCreationObserver
        );
    }
}
