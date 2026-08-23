//! Test-only access to a Worker host-bridge task body.
//!
//! These helpers exist only for domain authorization/result unit tests. Tests
//! covering a complete HTML task use `page_selected_task_test_harness`.

use crate::page_task_queue::{
    RendererPageNetworkingOwner, RendererPageNetworkingTask, RendererPageReadyDescriptor,
    RendererPageSchedulerTask, RendererPageWorkerHostBridgeTask,
};

use super::PageVm;

impl PageVm {
    pub(crate) fn take_worker_host_bridge_body_task_for_test(
        &mut self,
    ) -> Option<RendererPageWorkerHostBridgeTask> {
        let sources = self.page_task_executor_sources_for_test();
        let task = sources.take_scheduler_task_for_executor_test(|descriptor| {
            matches!(
                descriptor,
                RendererPageReadyDescriptor::Networking {
                    owner: RendererPageNetworkingOwner::WorkerHostBridge(_),
                    ..
                }
            )
        })?;
        let RendererPageSchedulerTask::Networking(RendererPageNetworkingTask::WorkerHostBridge(
            task,
        )) = task
        else {
            unreachable!("WorkerHostBridge descriptor must dequeue its own Networking variant")
        };
        Some(task)
    }
}
