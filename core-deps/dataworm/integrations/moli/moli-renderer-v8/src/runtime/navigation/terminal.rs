//! Navigation-owner terminal facts.
//!
//! These values describe what the navigation owner produced. They do not know
//! about lifecycle milestones, Page creation, scheduling, or protocol waits.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::runtime) enum PageNavigationOwnerFailure {
    TooManyChainedLocationNavigations { context: &'static str },
}

impl std::fmt::Display for PageNavigationOwnerFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooManyChainedLocationNavigations { context } => {
                write!(f, "too many chained location navigations while {context}")
            }
        }
    }
}
