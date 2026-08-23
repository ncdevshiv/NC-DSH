//! Task-end completion for one child modulepreload fetch start.
//!
//! HTML starts a modulepreload graph while processing the connected `link` and
//! reports the asynchronous result later through a load/error event. Blink's
//! `ModulePreloadIfNeeded()` follows that shape directly; it does not define a
//! separately named modulepreload-start task.
//!
//! Moli has an additional P3 owner/network handoff. Each accepted start
//! is independently scheduler-visible and can reserve or join module-map
//! state, start one request, or publish a typed terminal follow-up. The later
//! Networking terminal and link event remain separate tasks. As long as this
//! handoff remains a selected Page task, its exact current-owner body receives
//! the ordinary task-end checkpoint. Folding the handoff back into link
//! processing must also fold this completion authority; otherwise the same
//! HTML algorithm would gain an extra checkpoint.
//!
//! A stale root Document/child Document/realm claim never enters or
//! checkpoints the replacement realm.

use crate::page_task_queue::{
    PageModulepreloadStartDocumentEffect, PageModulepreloadStartTurnAction,
};

use super::{IntoPageTaskCompletion, PageTaskCompletion};

impl IntoPageTaskCompletion for PageModulepreloadStartTurnAction {
    fn into_page_task_completion(self) -> PageTaskCompletion {
        match self.document_effect {
            PageModulepreloadStartDocumentEffect::AppliedToCurrentOwner { .. } => {
                PageTaskCompletion::CheckpointOnly
            }
            PageModulepreloadStartDocumentEffect::DiscardedStaleOwner { .. } => {
                PageTaskCompletion::NoCompletion
            }
        }
    }
}
