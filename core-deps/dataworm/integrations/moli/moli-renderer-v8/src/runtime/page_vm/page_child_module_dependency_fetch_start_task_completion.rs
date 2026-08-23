//! Task-end completion for one child static-module dependency fetch start.
//!
//! Fetching a module graph recursively starts dependency fetches from the
//! graph algorithm. Moli inserts an additional owner/network handoff:
//! each dependency start is a concrete, independently selected Page task. Its
//! body only reserves or joins module-map state, starts a network request, or
//! publishes a typed pre-start failure terminal. The later Networking
//! completion, module linking, and script evaluation remain separate work.
//!
//! HTML and Blink start a new dependency fetch inside module-graph processing;
//! they do not define this handoff as a separate task. The checkpoint here is
//! justified only because Moli's existing P3 carrier is independently
//! scheduler-visible. If that carrier is folded back into the graph task, its
//! completion authority must be folded with it so no extra checkpoint remains.
//!
//! This component therefore gives the production selected-task dispatcher the
//! sole ordinary checkpoint for a current-owner body. A stale root
//! Document/child Document/realm claim never enters the replacement realm.

use crate::page_task_queue::{
    PageChildModuleDependencyFetchStartTargetEffect, PageChildModuleDependencyFetchStartTurnAction,
};

use super::{IntoPageTaskCompletion, PageTaskCompletion};

impl IntoPageTaskCompletion for PageChildModuleDependencyFetchStartTurnAction {
    fn into_page_task_completion(self) -> PageTaskCompletion {
        match self.target_effect {
            PageChildModuleDependencyFetchStartTargetEffect::AppliedToCurrentOwner { .. } => {
                PageTaskCompletion::CheckpointOnly
            }
            PageChildModuleDependencyFetchStartTargetEffect::DiscardedStaleOwner { .. } => {
                PageTaskCompletion::NoCompletion
            }
        }
    }
}
