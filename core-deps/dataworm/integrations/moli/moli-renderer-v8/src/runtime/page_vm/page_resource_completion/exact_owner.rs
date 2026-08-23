use crate::page_resource_completion::RendererPageResourceCompletionOwner;

use super::super::PageVm;

impl PageVm {
    /// Resolves the complete owner currently installed for the same local
    /// target shape as `expected`.
    ///
    /// This is the one shared exact-owner projection used by resource lane
    /// executors and source-local scheduler eligibility. It observes current
    /// state only; it never applies a terminal or advances work.
    pub(in crate::runtime::page_vm) fn current_page_resource_completion_owner(
        &self,
        expected: RendererPageResourceCompletionOwner,
    ) -> Option<RendererPageResourceCompletionOwner> {
        self.vm().current_page_resource_completion_owner_for_root(
            self.document_lifecycle.identity().document,
            expected,
        )
    }
}
