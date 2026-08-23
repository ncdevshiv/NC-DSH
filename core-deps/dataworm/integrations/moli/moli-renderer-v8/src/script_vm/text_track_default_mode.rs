use anyhow::{Result, anyhow};

use super::ScriptVm;
use crate::{
    page_task_queue::{
        RendererPageTextTrackDefaultModeOwner, RendererPageTextTrackDefaultModeTaskId,
        RendererPageTextTrackDefaultModeTaskKind,
    },
    runtime::AuthorizedCurrentPageTextTrackDefaultMode,
};

impl ScriptVm {
    pub(crate) fn current_pending_text_track_default_mode_owner(
        &self,
        task_id: RendererPageTextTrackDefaultModeTaskId,
        root_document: crate::runtime::RendererDocumentToken,
    ) -> Option<(
        RendererPageTextTrackDefaultModeOwner,
        RendererPageTextTrackDefaultModeTaskKind,
    )> {
        let (target, kind) = self
            ._context_host
            .borrow()
            .current_pending_text_track_default_mode_task(task_id)?;
        Some((
            RendererPageTextTrackDefaultModeOwner::new(root_document, target),
            kind,
        ))
    }

    /// Apply one authorized automatic-selection task body.
    ///
    /// Changing the mode can publish a later Networking task, but this body
    /// does not dispatch a public callback. The selected DOM-manipulation
    /// dispatcher owns the ordinary task-end checkpoint.
    pub(crate) fn apply_current_text_track_default_mode_body(
        &mut self,
        authorization: AuthorizedCurrentPageTextTrackDefaultMode,
    ) -> Result<bool> {
        let task = authorization.into_task();
        let owner = task.owner();
        self.with_default_context_scope(|scope, host_ptr| {
            unsafe { &mut *host_ptr }
                .apply_authorized_text_track_default_mode(
                    scope,
                    host_ptr,
                    task.task_id(),
                    owner.target(),
                    task.kind(),
                )
                .ok_or_else(|| {
                    anyhow!("authorized text-track default-mode task lost its exact payload")
                })
        })
    }

    pub(crate) fn discard_stale_text_track_default_mode_task(
        &mut self,
        task_id: RendererPageTextTrackDefaultModeTaskId,
    ) -> bool {
        self._context_host
            .borrow_mut()
            .discard_pending_text_track_default_mode_task(task_id)
    }
}
