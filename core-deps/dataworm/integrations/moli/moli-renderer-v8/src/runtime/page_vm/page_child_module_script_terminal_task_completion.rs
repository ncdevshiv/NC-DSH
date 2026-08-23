//! Task-end completion for one child module-map terminal batch.
//!
//! A terminal body may compile fetched source and publish typed dependency,
//! graph-ready, or failure successors. It does not evaluate the module,
//! dispatch a callback, or execute any successor. The selected terminal is an
//! ordinary Moli Page task, so a current exact owner receives only its
//! task-end checkpoint. Callback child/runtime reconciliation would grant this
//! state transition execution authority it does not own.
//!
//! A stale root Document, child Document, or realm is only discarded. It must
//! not enter the replacement realm to manufacture a checkpoint.

use crate::page_task_queue::{
    PageChildModuleScriptTerminalTargetEffect, PageChildModuleScriptTerminalTurnAction,
};

use super::{IntoPageTaskCompletion, PageTaskCompletion};

impl IntoPageTaskCompletion for PageChildModuleScriptTerminalTurnAction {
    fn into_page_task_completion(self) -> PageTaskCompletion {
        match self.target_effect {
            PageChildModuleScriptTerminalTargetEffect::AppliedToCurrentOwner { .. } => {
                PageTaskCompletion::CheckpointOnly
            }
            PageChildModuleScriptTerminalTargetEffect::DiscardedStaleOwner { .. } => {
                PageTaskCompletion::NoCompletion
            }
        }
    }
}
