//! Test-only access to a `ModuleReaction` task body.
//!
//! These helpers exist only for domain authorization/result unit tests. Tests
//! covering task-end settlement use `page_selected_task_test_harness`.

use crate::page_task_queue::{
    RendererPageModuleReactionTask, RendererPageReadyDescriptor, RendererPageSchedulerTask,
};

use super::PageVm;

impl PageVm {
    pub(crate) fn take_module_reaction_body_task_for_test(
        &mut self,
    ) -> Option<RendererPageModuleReactionTask> {
        let sources = self.page_task_executor_sources_for_test();
        let task = sources.take_scheduler_task_for_executor_test(|descriptor| {
            matches!(
                descriptor,
                RendererPageReadyDescriptor::ModuleReaction { .. }
            )
        })?;
        let RendererPageSchedulerTask::ModuleReaction(task) = task else {
            unreachable!("ModuleReaction descriptor must dequeue its own task variant")
        };
        Some(task)
    }
}
