//! Task-end boundary for an exact child frame-owner `load` delivery.
//!
//! The HostLoad body synchronously dispatches the embedding element's `load`
//! callback, matching Blink's `LocalDOMWindow::DispatchLoadEvent()` handoff to
//! `FrameOwner::DispatchLoad()`. Microtasks are an ordinary selected-task
//! boundary: the body leaves them pending and this module maps the typed
//! post-execution result to the one central Page completion authority.

use crate::page_task_queue::{PageChildHostLoadTargetEffect, PageChildHostLoadTurnAction};

use super::{IntoPageTaskCompletion, PageTaskCompletion};

impl IntoPageTaskCompletion for PageChildHostLoadTurnAction {
    fn into_page_task_completion(self) -> PageTaskCompletion {
        match self.target_effect {
            PageChildHostLoadTargetEffect::CallbackDispatchedToCurrentOwner => {
                PageTaskCompletion::CallbackCompletion
            }
            PageChildHostLoadTargetEffect::ConsumedCurrentOwnerWithoutCallback
            | PageChildHostLoadTargetEffect::FailedForCurrentOwner => {
                // The exact current task entered the parent Window realm. The
                // old context-scope boundary always completed its ordinary
                // checkpoint, even if the target disappeared or the body
                // returned an error. Without a dispatched callback there is
                // no child/runtime follow-up to reconcile.
                PageTaskCompletion::CheckpointOnly
            }
            PageChildHostLoadTargetEffect::DiscardedStaleOwner { .. } => {
                // A stale ticket never entered the replacement realm and must
                // not manufacture a checkpoint there.
                PageTaskCompletion::NoCompletion
            }
        }
    }
}
