use crate::page_task_queue::{
    PageParserAsyncModuleAdmissionTargetEffect, PageParserAsyncModuleAdmissionTurnAction,
};

use super::{IntoPageTaskCompletion, PageTaskCompletion};

impl IntoPageTaskCompletion for PageParserAsyncModuleAdmissionTurnAction {
    fn into_page_task_completion(self) -> PageTaskCompletion {
        match self.target_effect() {
            // Admission mutates only the exact parser PendingScript owner. It
            // dispatches no callback and does not synchronously consume the
            // graph continuation it may publish.
            PageParserAsyncModuleAdmissionTargetEffect::AdmittedToCurrentOwner
            | PageParserAsyncModuleAdmissionTargetEffect::RejectedByCurrentOwner => {
                PageTaskCompletion::CheckpointOnly
            }
            PageParserAsyncModuleAdmissionTargetEffect::DiscardedStaleOwner => {
                PageTaskCompletion::NoCompletion
            }
        }
    }
}
