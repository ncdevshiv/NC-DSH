//! Task-end boundary for exact child default-realm materialization.
//!
//! Realm construction and stored document-start replay are one selected
//! child-frame task. The execution-produced target effect chooses between a
//! plain checkpoint and the script-aware checkpoint that also synchronizes
//! child records. Stale work never enters the replacement realm.

use anyhow::Result;

use crate::page_task_queue::{
    PageChildRealmMaterializationTargetEffect, PageChildRealmMaterializationTurnAction,
};

use super::PageVm;

enum PageChildRealmMaterializationTaskEnd {
    CheckpointOnly,
    DocumentStartScriptCheckpoint,
}

enum PageChildRealmMaterializationCompletionBoundary {
    Complete(PageChildRealmMaterializationTaskEnd),
    DiscardedStale,
}

impl PageChildRealmMaterializationTurnAction {
    fn into_completion_boundary(self) -> PageChildRealmMaterializationCompletionBoundary {
        match self.target_effect {
            PageChildRealmMaterializationTargetEffect::MaterializedCurrentOwnerAfterDocumentStartScript => {
                PageChildRealmMaterializationCompletionBoundary::Complete(
                    PageChildRealmMaterializationTaskEnd::DocumentStartScriptCheckpoint,
                )
            }
            PageChildRealmMaterializationTargetEffect::MaterializedCurrentOwnerWithoutDocumentStartScript
            | PageChildRealmMaterializationTargetEffect::FailedCurrentOwner
            | PageChildRealmMaterializationTargetEffect::CurrentOwnerHadNoPendingRequest => {
                PageChildRealmMaterializationCompletionBoundary::Complete(
                    PageChildRealmMaterializationTaskEnd::CheckpointOnly,
                )
            }
            PageChildRealmMaterializationTargetEffect::IgnoredStaleOwner { .. } => {
                PageChildRealmMaterializationCompletionBoundary::DiscardedStale
            }
        }
    }
}

impl PageVm {
    pub(super) fn finish_selected_page_child_realm_materialization(
        &mut self,
        action: PageChildRealmMaterializationTurnAction,
    ) -> Result<()> {
        match action.into_completion_boundary() {
            PageChildRealmMaterializationCompletionBoundary::Complete(
                PageChildRealmMaterializationTaskEnd::CheckpointOnly,
            ) => self.finish_selected_page_task_checkpoint(),
            PageChildRealmMaterializationCompletionBoundary::Complete(
                PageChildRealmMaterializationTaskEnd::DocumentStartScriptCheckpoint,
            ) => self
                .vm_mut()
                .finish_child_realm_materialization_script_task_checkpoint(),
            PageChildRealmMaterializationCompletionBoundary::DiscardedStale => Ok(()),
        }
    }
}
