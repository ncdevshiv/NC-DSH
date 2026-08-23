//! Task-end completion for Page-owned internal-loading tasks.
//!
//! A current declarative-refresh task only updates navigation state. It does
//! not dispatch an author callback or own generic child/runtime follow-up, so
//! both an activated refresh and a no-op refresh superseded by another
//! navigation receive `CheckpointOnly`.
//!
//! A task stamped by a detached Document receives `NoCompletion`. Chromium
//! achieves the equivalent boundary by canceling its weak, cancellable
//! `kInternalLoading` task when the old Document detaches; Moli's stable
//! source can expose the stale payload later and therefore authorizes it here.

use crate::page_task_queue::{PageInternalLoadingTargetEffect, PageInternalLoadingTurnAction};

use super::{IntoPageTaskCompletion, PageTaskCompletion};

impl IntoPageTaskCompletion for PageInternalLoadingTurnAction {
    fn into_page_task_completion(self) -> PageTaskCompletion {
        match self.target_effect {
            PageInternalLoadingTargetEffect::AppliedToCurrentOwner { .. } => {
                PageTaskCompletion::CheckpointOnly
            }
            PageInternalLoadingTargetEffect::DiscardedStaleOwner { .. } => {
                PageTaskCompletion::NoCompletion
            }
        }
    }
}
