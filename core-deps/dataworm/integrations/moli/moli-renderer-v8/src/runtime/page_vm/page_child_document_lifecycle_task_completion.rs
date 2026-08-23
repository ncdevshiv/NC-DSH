//! Task-end boundary for an exact child `Document` lifecycle action.
//!
//! Interactive, `DOMContentLoaded`, and complete transitions are ordinary
//! selected child-frame tasks. Their bodies synchronously dispatch the
//! corresponding document event when a wrapper still exists, but deliberately
//! leave Promise reactions pending. This module maps that execution-produced
//! fact to the sole Page task-completion authority.
//!
//! This does not govern the parser-owned direct-successor boundary where the
//! final deferred script completes parsing and synchronously dispatches DCL in
//! the same task. That separately typed parser completion path must remain
//! contiguous and must not be turned into another scheduler task here.

use crate::page_task_queue::{
    PageChildDocumentLifecycleTargetEffect, PageChildDocumentLifecycleTurnAction,
};

use super::{IntoPageTaskCompletion, PageTaskCompletion};

impl IntoPageTaskCompletion for PageChildDocumentLifecycleTurnAction {
    fn into_page_task_completion(self) -> PageTaskCompletion {
        match self.target_effect {
            PageChildDocumentLifecycleTargetEffect::EventDispatchedToCurrentOwner => {
                PageTaskCompletion::CallbackCompletion
            }
            PageChildDocumentLifecycleTargetEffect::ConsumedCurrentOwnerWithoutEvent
            | PageChildDocumentLifecycleTargetEffect::FailedForCurrentOwner => {
                // The exact current task entered its child Window realm. The
                // previous realm-scope helper always completed an ordinary
                // checkpoint even if no event wrapper survived or execution
                // failed, but there is no callback follow-up to reconcile.
                PageTaskCompletion::CheckpointOnly
            }
            PageChildDocumentLifecycleTargetEffect::DiscardedStaleOwner { .. } => {
                // A stale action never entered the replacement realm and must
                // not manufacture a checkpoint there.
                PageTaskCompletion::NoCompletion
            }
        }
    }
}
