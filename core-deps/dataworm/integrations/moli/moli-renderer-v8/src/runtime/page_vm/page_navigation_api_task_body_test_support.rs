//! Test-only access to a Navigation API task body.
//!
//! These helpers exist only for domain authorization/result unit tests. Tests
//! covering a complete HTML task use `page_selected_task_test_harness`.

use crate::page_task_queue::{
    RendererPageNavigationAndTraversalTask, RendererPageReadyDescriptor, RendererPageSchedulerTask,
};

use super::PageVm;

impl PageVm {
    pub(crate) fn take_navigation_api_body_task_for_test(
        &mut self,
    ) -> Option<crate::page_task_queue::RendererPageNavigationApiTask> {
        let sources = self.page_task_executor_sources_for_test();
        let task = sources.take_scheduler_task_for_executor_test(|descriptor| {
            matches!(
                descriptor,
                RendererPageReadyDescriptor::NavigationAndTraversal {
                    head:
                        crate::page_task_queue::RendererPageNavigationAndTraversalHead::NavigationApi {
                            ..
                        },
                    ..
                }
            )
        })?;
        let RendererPageSchedulerTask::NavigationAndTraversal(
            RendererPageNavigationAndTraversalTask::NavigationApi(task),
        ) = task
        else {
            unreachable!("NavigationApi descriptor must dequeue its own source variant")
        };
        Some(task)
    }
}
