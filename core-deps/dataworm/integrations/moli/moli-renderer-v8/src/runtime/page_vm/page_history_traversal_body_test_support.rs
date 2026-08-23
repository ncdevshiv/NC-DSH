//! Test-only access to a HistoryTraversal task body.
//!
//! These helpers exist only for domain authorization/result unit tests. Tests
//! covering a complete HTML task use `page_selected_task_test_harness`.

use crate::page_task_queue::{
    RendererPageNavigationAndTraversalTask, RendererPageReadyDescriptor, RendererPageSchedulerTask,
};

use super::PageVm;

impl PageVm {
    pub(crate) fn take_history_traversal_body_task_for_test(
        &mut self,
    ) -> Option<crate::page_task_queue::RendererPageHistoryTraversalTask> {
        let sources = self.page_task_executor_sources_for_test();
        let task = sources.take_scheduler_task_for_executor_test(|descriptor| {
            matches!(
                descriptor,
                RendererPageReadyDescriptor::NavigationAndTraversal {
                    head:
                        crate::page_task_queue::RendererPageNavigationAndTraversalHead::HistoryTraversal {
                            ..
                        },
                    ..
                }
            )
        })?;
        let RendererPageSchedulerTask::NavigationAndTraversal(
            RendererPageNavigationAndTraversalTask::HistoryTraversal(task),
        ) = task
        else {
            unreachable!("HistoryTraversal descriptor must dequeue its own source variant")
        };
        Some(task)
    }
}
