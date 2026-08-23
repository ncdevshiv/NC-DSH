use crate::{
    page_task_queue::{
        RendererPageChildClassicScriptSourceLoadTarget, RendererPageChildFrameTaskTarget,
    },
    runtime::AuthorizedCurrentPageChildClassicScriptSourceLoad,
};

use super::ScriptVm;

impl ScriptVm {
    pub(crate) fn current_child_classic_source_load_target(
        &self,
        expected: RendererPageChildClassicScriptSourceLoadTarget,
    ) -> Option<RendererPageChildClassicScriptSourceLoadTarget> {
        self._context_host
            .borrow()
            .current_child_classic_source_load_target(expected)
    }

    pub(crate) fn settle_stale_child_classic_source_load(
        &mut self,
        task: &crate::frame_owner_model::FrameDocumentClassicScriptSourceLoadTask,
    ) {
        let _ = self
            ._context_host
            .borrow_mut()
            .fail_child_classic_source_load_before_start(
                task,
                "child classic source-load realm retired before fetch start",
            );
    }

    pub(crate) fn apply_current_child_classic_source_load(
        &mut self,
        authorization: AuthorizedCurrentPageChildClassicScriptSourceLoad,
    ) -> crate::frame_owner_model::FrameDocumentClassicScriptSourceLoadStartOutcome {
        let task = authorization.into_task();
        let RendererPageChildFrameTaskTarget::ClassicScriptSourceLoad(target) =
            task.owner().target()
        else {
            unreachable!("classic-source executor received another child-frame task kind")
        };
        let source_load = task.into_classic_script_source_load_task();
        let outcome = self
            ._context_host
            .borrow_mut()
            .start_current_child_classic_source_load_task(source_load);
        if outcome
            == crate::frame_owner_model::FrameDocumentClassicScriptSourceLoadStartOutcome::NetworkRequestStarted
        {
            tracing::debug!(
                owner = ?target.document_owner(),
                realm_id = ?target.realm_id(),
                child_handle = ?target.child_handle(),
                script_handle = ?target.script_handle(),
                "child classic source load handed to owner network bridge"
            );
        }
        outcome
    }

    /// Apply one classic source-start body from the production child-frame
    /// family in low-level semantic fixtures. This does not submit the selected
    /// Page task's completion. A real source start must retain the exact child
    /// Document resource authority; there is no ambient main-Document fallback.
    #[cfg(test)]
    pub(crate) fn run_child_classic_source_load_body_for_test(
        &mut self,
    ) -> anyhow::Result<Option<()>> {
        let source = self
            ._page_task_residence_for_executor_test
            .as_ref()
            .expect("classic-source executor fixture must retain its production Page source")
            .task_sources();
        let Some(task) = source.take_scheduler_task_for_executor_test(|descriptor| {
            matches!(
                descriptor,
                crate::page_task_queue::RendererPageReadyDescriptor::ChildFrameTask {
                    owner,
                    ..
                } if matches!(
                    owner.target(),
                    RendererPageChildFrameTaskTarget::ClassicScriptSourceLoad(_)
                )
            )
        }) else {
            return Ok(None);
        };
        let crate::page_task_queue::RendererPageSchedulerTask::ChildFrameTask(task) = task else {
            unreachable!("child-frame descriptor must dequeue its own family source")
        };
        let owner = task.owner();
        let RendererPageChildFrameTaskTarget::ClassicScriptSourceLoad(target) = owner.target()
        else {
            unreachable!("classic-source selector must only dequeue classic source tasks")
        };
        if self.current_child_classic_source_load_target(target) == Some(target) {
            let _outcome = self.apply_current_child_classic_source_load(
                AuthorizedCurrentPageChildClassicScriptSourceLoad::new_for_executor_test(task),
            );
            return Ok(Some(()));
        }
        let source_load = task.into_classic_script_source_load_task();
        self.settle_stale_child_classic_source_load(&source_load);
        Ok(Some(()))
    }
}
