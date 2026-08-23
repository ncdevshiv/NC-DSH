//! Test-only access to a ServiceWorker client-message task body.
//!
//! These helpers exist only for domain authorization/result unit tests. Tests
//! covering a complete HTML task use `page_selected_task_test_harness`.

use crate::page_task_queue::{
    RendererPageReadyDescriptor, RendererPageSchedulerTask,
    RendererPageServiceWorkerClientMessageTask,
};

use super::PageVm;

impl PageVm {
    pub(crate) fn take_service_worker_client_message_body_task_for_test(
        &mut self,
    ) -> Option<RendererPageServiceWorkerClientMessageTask> {
        let sources = self.page_task_executor_sources_for_test();
        let task = sources.take_scheduler_task_for_executor_test(|descriptor| {
            matches!(
                descriptor,
                RendererPageReadyDescriptor::ServiceWorkerClientMessage { .. }
            )
        })?;
        let RendererPageSchedulerTask::ServiceWorkerClientMessage(task) = task else {
            unreachable!("ServiceWorker client-message descriptor must dequeue its own source")
        };
        Some(task)
    }
}
