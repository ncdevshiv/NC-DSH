use anyhow::Result;

use super::ScriptVm;
use crate::page_task_queue::{
    RendererPageViewTransitionUpdateOwner, RendererPageViewTransitionUpdateTaskId,
};

impl ScriptVm {
    pub(crate) fn current_view_transition_update_owner(
        &self,
        expected: RendererPageViewTransitionUpdateOwner,
        root_document: crate::runtime::RendererDocumentToken,
    ) -> Option<RendererPageViewTransitionUpdateOwner> {
        self.current_window_task_target(expected.target())
            .map(|target| RendererPageViewTransitionUpdateOwner::new(root_document, target))
    }

    pub(crate) fn apply_current_view_transition_update_body(
        &mut self,
        authorization: crate::runtime::AuthorizedCurrentPageViewTransitionUpdate,
    ) -> Result<bool> {
        let task = authorization.into_task();
        let owner = task.owner();
        let task_id = task.task_id();
        self.with_default_context_scope(|scope, host_ptr| {
            Ok(unsafe { &mut *host_ptr }
                .invoke_authorized_view_transition_update_callback(scope, task_id, owner))
        })
    }

    pub(crate) fn discard_stale_view_transition_update(
        &mut self,
        task_id: RendererPageViewTransitionUpdateTaskId,
        owner: RendererPageViewTransitionUpdateOwner,
    ) {
        self._context_host
            .borrow_mut()
            .discard_view_transition_update_callback(task_id, owner);
    }
}
