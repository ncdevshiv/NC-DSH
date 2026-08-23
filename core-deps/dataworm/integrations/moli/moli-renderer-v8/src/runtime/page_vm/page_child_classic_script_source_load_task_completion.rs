//! Task-end completion for one child classic-script source start.
//!
//! Blink starts a parser classic fetch from script preparation and leaves the
//! renderer scheduler to complete the enclosing task checkpoint. Moli
//! has an additional owner/network handoff: source start is a concrete,
//! independently selected Page task. Its body only starts the request or
//! publishes a typed pre-start failure successor. This component gives the
//! production dispatcher sole ownership of that task's ordinary checkpoint;
//! a stale exact-owner claim never enters the replacement realm.

use crate::page_task_queue::{
    PageChildClassicScriptSourceLoadTargetEffect, PageChildClassicScriptSourceLoadTurnAction,
};

use super::{IntoPageTaskCompletion, PageTaskCompletion};

impl IntoPageTaskCompletion for PageChildClassicScriptSourceLoadTurnAction {
    fn into_page_task_completion(self) -> PageTaskCompletion {
        match self.target_effect {
            PageChildClassicScriptSourceLoadTargetEffect::NetworkRequestStartedForCurrentOwner
            | PageChildClassicScriptSourceLoadTargetEffect::RejectedBeforeNetworkStartForCurrentOwner => {
                PageTaskCompletion::CheckpointOnly
            }
            PageChildClassicScriptSourceLoadTargetEffect::DiscardedStaleOwner { .. } => {
                PageTaskCompletion::NoCompletion
            }
        }
    }
}
