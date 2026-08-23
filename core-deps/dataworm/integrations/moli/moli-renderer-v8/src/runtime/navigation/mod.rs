mod page_creation_resolution;
mod page_creation_watch;
mod terminal;

pub(super) use page_creation_resolution::{PageCreationResolution, PageCreationRetirement};
pub(super) use page_creation_watch::{
    PageCreationNavigationFailureObserver, PageCreationNavigationFailurePublication,
    PageCreationNavigationFailurePublisher, page_creation_navigation_failure_scope,
};
pub(super) use terminal::PageNavigationOwnerFailure;
