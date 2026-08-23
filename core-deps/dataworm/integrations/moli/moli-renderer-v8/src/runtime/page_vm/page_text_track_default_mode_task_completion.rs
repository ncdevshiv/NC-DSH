//! Completion classification for automatic text-track mode selection.
//!
//! Automatic selection can change the track mode and publish a later
//! Networking task, but it does not dispatch a public callback in this task.
//! It therefore owns one ordinary task-end checkpoint and no callback
//! reconciliation.

use crate::page_task_queue::{
    PageTextTrackDefaultModeTargetEffect, PageTextTrackDefaultModeTurnAction,
};

use super::{IntoPageTaskCompletion, PageTaskCompletion};

impl IntoPageTaskCompletion for PageTextTrackDefaultModeTurnAction {
    fn into_page_task_completion(self) -> PageTaskCompletion {
        match self.target_effect {
            PageTextTrackDefaultModeTargetEffect::AppliedToCurrentOwner
            | PageTextTrackDefaultModeTargetEffect::CurrentOwnerNoLongerEligible => {
                PageTaskCompletion::CheckpointOnly
            }
            PageTextTrackDefaultModeTargetEffect::DiscardedStaleOwner { .. } => {
                PageTaskCompletion::NoCompletion
            }
        }
    }
}
