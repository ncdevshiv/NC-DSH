//! Test-only access to a stylesheet Networking task body.
//!
//! This helper exists only for domain authorization/result unit tests. Tests
//! covering a complete HTML task use `page_selected_task_test_harness` so the
//! production selected-task dispatcher remains the sole completion authority.

use crate::page_task_queue::{
    RendererPageNetworkingOwner, RendererPageNetworkingTask, RendererPageReadyDescriptor,
    RendererPageSchedulerTask, RendererPageStylesheetNetworkingTask,
};

use super::PageVm;

impl PageVm {
    pub(crate) fn take_stylesheet_networking_body_task_for_test(
        &mut self,
    ) -> Option<RendererPageStylesheetNetworkingTask> {
        let sources = self.page_task_executor_sources_for_test();
        let task = sources.take_scheduler_task_for_executor_test(|descriptor| {
            matches!(
                descriptor,
                RendererPageReadyDescriptor::Networking {
                    owner: RendererPageNetworkingOwner::StylesheetCompletion(_),
                    ..
                }
            )
        })?;
        let RendererPageSchedulerTask::Networking(
            RendererPageNetworkingTask::StylesheetCompletion(task),
        ) = task
        else {
            unreachable!("StylesheetCompletion descriptor must dequeue its own Networking variant")
        };
        Some(task)
    }
}
