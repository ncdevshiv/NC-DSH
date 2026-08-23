//! Task-end completion for one child parser-module graph start.
//!
//! Blink starts a parser module graph from script preparation and leaves the
//! renderer scheduler to perform the enclosing task checkpoint. Moli
//! has an additional owner/network handoff: the graph start is a concrete,
//! independently selected Page task. Its body remains state-only, and this
//! component gives the production dispatcher sole ownership of that task's
//! ordinary checkpoint. A stale exact-owner claim never enters the current
//! realm merely to manufacture a checkpoint.

use crate::page_task_queue::{
    PageChildParserModuleRootStartTargetEffect, PageChildParserModuleRootStartTurnAction,
};

use super::{IntoPageTaskCompletion, PageTaskCompletion};

impl IntoPageTaskCompletion for PageChildParserModuleRootStartTurnAction {
    fn into_page_task_completion(self) -> PageTaskCompletion {
        match self.target_effect {
            PageChildParserModuleRootStartTargetEffect::ConsumedByCurrentOwner => {
                PageTaskCompletion::CheckpointOnly
            }
            PageChildParserModuleRootStartTargetEffect::DiscardedStaleOwner { .. } => {
                PageTaskCompletion::NoCompletion
            }
        }
    }
}
