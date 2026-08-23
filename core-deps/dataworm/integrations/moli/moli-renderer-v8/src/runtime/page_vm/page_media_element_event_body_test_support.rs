//! Test-only access to a media-element event task body.
//!
//! These helpers exist only for domain authorization/result unit tests. Tests
//! covering a complete HTML task use `page_selected_task_test_harness`.

use crate::page_task_queue::{
    RendererPageMediaElementEventTask, RendererPageReadyDescriptor, RendererPageSchedulerTask,
};

use super::PageVm;

impl PageVm {
    pub(crate) fn take_media_element_event_body_task_for_test(
        &mut self,
    ) -> Option<RendererPageMediaElementEventTask> {
        let sources = self.page_task_executor_sources_for_test();
        let task = sources.take_scheduler_task_for_executor_test(|descriptor| {
            matches!(
                descriptor,
                RendererPageReadyDescriptor::MediaElementEvent { .. }
            )
        })?;
        let RendererPageSchedulerTask::MediaElementEvent(task) = task else {
            unreachable!("media-element descriptor must dequeue its own source")
        };
        Some(task)
    }
}
