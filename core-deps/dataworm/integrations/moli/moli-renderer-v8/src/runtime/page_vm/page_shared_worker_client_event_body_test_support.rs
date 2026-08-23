//! Test-only access to a SharedWorker client-event task body.
//!
//! These helpers exist only for domain authorization/result unit tests. Tests
//! covering a complete HTML task use `page_selected_task_test_harness`.

use crate::page_task_queue::{
    RendererPageReadyDescriptor, RendererPageSchedulerTask, RendererPageSharedWorkerClientEventTask,
};

use super::PageVm;

impl PageVm {
    pub(crate) fn take_shared_worker_client_event_body_task_for_test(
        &mut self,
    ) -> Option<RendererPageSharedWorkerClientEventTask> {
        let sources = self.page_task_executor_sources_for_test();
        let task = sources.take_scheduler_task_for_executor_test(|descriptor| {
            matches!(
                descriptor,
                RendererPageReadyDescriptor::SharedWorkerClientEvent { .. }
            )
        })?;
        let RendererPageSchedulerTask::SharedWorkerClientEvent(task) = task else {
            unreachable!("SharedWorker descriptor must dequeue its own source")
        };
        Some(task)
    }
}
