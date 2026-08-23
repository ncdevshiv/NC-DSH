//! Task-end boundary for the heterogeneous child `DocumentScriptReady` lane.
//!
//! Classic and module bodies return whether script or event code ran, and this
//! module selects the sole enclosing HTML task-end completion. A child module
//! may already have performed its algorithm-required error-handling checkpoint;
//! that does not replace this selected-task boundary.

use crate::page_task_queue::{
    PageChildDocumentScriptReadyTargetEffect, PageChildDocumentScriptReadyTurnAction,
};

use super::PageTaskCompletion;

pub(super) enum PageChildDocumentScriptReadyCompletionBoundary {
    Complete(PageTaskCompletion),
    DiscardedStale,
}

impl PageChildDocumentScriptReadyTurnAction {
    pub(super) fn into_completion_boundary(self) -> PageChildDocumentScriptReadyCompletionBoundary {
        match self.target_effect {
            PageChildDocumentScriptReadyTargetEffect::AppliedScriptOrEventToCurrentOwner {
                ..
            } => PageChildDocumentScriptReadyCompletionBoundary::Complete(
                PageTaskCompletion::CallbackCompletion,
            ),
            PageChildDocumentScriptReadyTargetEffect::AppliedWithoutScriptOrEventToCurrentOwner {
                ..
            } => PageChildDocumentScriptReadyCompletionBoundary::Complete(
                PageTaskCompletion::CheckpointOnly,
            ),
            PageChildDocumentScriptReadyTargetEffect::DiscardedStaleOwner { .. } => {
                PageChildDocumentScriptReadyCompletionBoundary::DiscardedStale
            }
        }
    }
}
