use crate::{
    document_script_scheduler::MainParserAsyncModuleAdmission,
    page_task_queue::{
        PostParsePageOwnedWork, RendererPageMainDocumentRuntimeAdmissionError,
        RendererPageMainDocumentRuntimeRouteClosed,
    },
};

use super::HostScriptScheduler;

impl HostScriptScheduler {
    /// Publish concrete post-parse work through the producer already bound to
    /// the exact current main Document/runtime generation.
    pub(crate) fn enqueue_main_document_post_parse_work(
        &self,
        work: PostParsePageOwnedWork,
    ) -> Result<(), RendererPageMainDocumentRuntimeAdmissionError> {
        let producer = self
            .main_document_runtime_producer
            .as_ref()
            .ok_or(RendererPageMainDocumentRuntimeAdmissionError::RouteClosed)?;
        producer.send_post_parse_work_when_ready(work)
    }

    /// Transfer one parser async module into the exact main-runtime admission
    /// source. The selected action then installs the shared `PendingScript`;
    /// this scheduler does not retain a parallel written-script queue.
    pub(crate) fn enqueue_main_parser_async_module_admission(
        &self,
        admission: MainParserAsyncModuleAdmission,
    ) -> Result<(), RendererPageMainDocumentRuntimeRouteClosed> {
        let Some(producer) = self.main_document_runtime_producer.as_ref() else {
            return Err(RendererPageMainDocumentRuntimeRouteClosed);
        };
        producer.send_parser_async_module_admission(admission)
    }
}
