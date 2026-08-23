//! Test-only access to a DedicatedWorker task body.
//!
//! These helpers exist only for domain authorization/result unit tests. Tests
//! covering a complete HTML task must use `page_selected_task_test_harness`.

use crate::page_task_queue::{
    RendererPageDedicatedWorkerClientEventTask, RendererPageReadyDescriptor,
    RendererPageSchedulerTask,
};

use super::PageVm;

impl PageVm {
    pub(crate) fn take_dedicated_worker_client_event_body_task_for_test(
        &mut self,
    ) -> Option<RendererPageDedicatedWorkerClientEventTask> {
        let sources = self.page_task_executor_sources_for_test();
        let task = sources.take_scheduler_task_for_executor_test(|descriptor| {
            matches!(
                descriptor,
                RendererPageReadyDescriptor::DedicatedWorkerClientEvent { .. }
            )
        })?;
        let RendererPageSchedulerTask::DedicatedWorkerClientEvent(task) = task else {
            unreachable!("DedicatedWorker descriptor must dequeue its own source")
        };
        Some(task)
    }
}
