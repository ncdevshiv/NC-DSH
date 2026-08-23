//! Body-only test support for child `DocumentScriptReady`.
//!
//! Semantic task tests must use `PageSelectedTaskTestSelector` so the unique
//! production dispatcher performs task completion. This helper exists only for
//! focused body tests that intentionally prove Promise reactions remain
//! pending before that completion boundary.

use crate::page_task_queue::{
    PageChildDocumentScriptReadyTurnOutcome, RendererPageChildFrameTaskTarget,
    RendererPageReadyDescriptor, RendererPageSchedulerTask,
};

use super::PageVm;

impl PageVm {
    pub(crate) async fn run_page_child_document_script_ready_body_for_test(
        &mut self,
    ) -> anyhow::Result<Option<PageChildDocumentScriptReadyTurnOutcome>> {
        let task_sources = self.page_task_executor_sources_for_test();
        let Some(task) = task_sources.take_scheduler_task_for_executor_test(|descriptor| {
            matches!(
                descriptor,
                RendererPageReadyDescriptor::ChildFrameTask { owner, .. }
                    if matches!(
                        owner.target(),
                        RendererPageChildFrameTaskTarget::DocumentScriptReady(_)
                    )
            )
        }) else {
            return Ok(None);
        };
        let RendererPageSchedulerTask::ChildFrameTask(task) = task else {
            unreachable!("child-frame descriptor must dequeue its own family source")
        };
        self.apply_selected_page_child_document_script_ready_turn(task)
            .await
            .map(Some)
    }
}
