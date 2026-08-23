//! Test-only access to a ServiceWorker internal task body.
//!
//! These helpers exist only for domain authorization/result unit tests. Tests
//! covering a complete HTML task use `page_selected_task_test_harness`.

use crate::page_task_queue::{
    RendererPageReadyDescriptor, RendererPageSchedulerTask, RendererPageServiceWorkerInternalTask,
};

use super::PageVm;

impl PageVm {
    pub(crate) fn take_service_worker_internal_body_task_for_test(
        &mut self,
    ) -> Option<RendererPageServiceWorkerInternalTask> {
        let sources = self.page_task_executor_sources_for_test();
        let task = sources.take_scheduler_task_for_executor_test(|descriptor| {
            matches!(
                descriptor,
                RendererPageReadyDescriptor::ServiceWorkerInternal { .. }
            )
        })?;
        let RendererPageSchedulerTask::ServiceWorkerInternal(task) = task else {
            unreachable!("ServiceWorker internal descriptor must dequeue its own source")
        };
        Some(task)
    }
}
