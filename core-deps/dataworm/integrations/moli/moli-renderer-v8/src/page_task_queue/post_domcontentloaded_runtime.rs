use super::*;

#[cfg(test)]
impl PageTaskQueueTestHarness {
    /// Build a lightweight ScriptVm test fixture on the same typed Page source
    /// used by production. The fixture may inspect the production source
    /// harness, but it does not gain a PageVm-local fallback queue.
    pub(crate) fn owner_attached_runtime_page_task_sender_for_test(&self) -> RuntimePageTaskSender {
        self.residence.owner_attached_runtime_page_task_sender()
    }
}

impl PageTaskQueue {
    pub(crate) fn owner_attached_post_domcontentloaded_runtime_page_task_sender(
        &self,
        main_document_runtime: RendererPageMainDocumentRuntimeSender,
        main_parser_continuation: RendererPageMainParserContinuationSender,
        stylesheet: RendererPageStylesheetTaskSender,
        service_worker: RendererPageServiceWorkerTaskSender,
    ) -> RuntimePageTaskSender {
        self.page_runtime_task_source
            .owner_attached_runtime_page_task_sender(
                main_document_runtime,
                main_parser_continuation,
                stylesheet,
                service_worker,
            )
    }
}
