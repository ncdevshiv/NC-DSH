use anyhow::{Result, anyhow};

use super::ScriptVm;
use crate::page_task_queue::{
    RendererPageMediaElementEventOwner, RendererPageMediaElementEventTaskId,
    RendererPageMediaElementEventTaskKind,
};
use crate::runtime::AuthorizedCurrentPageMediaElementEvent;

impl ScriptVm {
    pub(crate) fn current_pending_media_element_event_owner(
        &self,
        task_id: RendererPageMediaElementEventTaskId,
        root_document: crate::runtime::RendererDocumentToken,
    ) -> Option<(
        RendererPageMediaElementEventOwner,
        RendererPageMediaElementEventTaskKind,
    )> {
        let (target, kind) = self
            ._context_host
            .borrow()
            .current_pending_media_element_event_task(task_id)?;
        Some((
            RendererPageMediaElementEventOwner::new(root_document, target),
            kind,
        ))
    }

    /// Apply only the callback-visible body of one authorized media-element
    /// event task.
    ///
    /// The selected Page-task dispatcher owns the task-end checkpoint, child
    /// synchronization, and runtime follow-up. Keeping those operations out of
    /// this helper prevents low-level semantic fixtures or nested callers from
    /// manufacturing an extra HTML task boundary.
    pub(crate) fn apply_current_media_element_event_body(
        &mut self,
        authorization: AuthorizedCurrentPageMediaElementEvent,
    ) -> Result<bool> {
        let task = authorization.into_task();
        let owner = task.owner();
        self.with_default_context_scope(|scope, host_ptr| {
            unsafe { &mut *host_ptr }
                .apply_authorized_media_element_event(
                    scope,
                    host_ptr,
                    task.task_id(),
                    owner.target(),
                    task.kind(),
                )
                .ok_or_else(|| anyhow!("authorized media-element event lost its exact payload"))
        })
    }

    pub(crate) fn discard_stale_media_element_event_task(
        &mut self,
        task_id: RendererPageMediaElementEventTaskId,
    ) -> bool {
        self._context_host
            .borrow_mut()
            .discard_pending_media_element_event_task(task_id)
    }
}
