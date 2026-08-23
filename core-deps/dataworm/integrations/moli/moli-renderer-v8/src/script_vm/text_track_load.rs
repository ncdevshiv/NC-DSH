use anyhow::{Result, anyhow};

use super::ScriptVm;
use crate::{
    page_task_queue::{
        RendererPageTextTrackLoadOwner, RendererPageTextTrackLoadTaskId,
        RendererPageTextTrackLoadTaskKind,
    },
    runtime::AuthorizedCurrentPageTextTrackLoad,
};

impl ScriptVm {
    pub(crate) fn current_pending_text_track_load_owner(
        &self,
        task_id: RendererPageTextTrackLoadTaskId,
        root_document: crate::runtime::RendererDocumentToken,
    ) -> Option<(
        RendererPageTextTrackLoadOwner,
        RendererPageTextTrackLoadTaskKind,
    )> {
        let (target, kind) = self
            ._context_host
            .borrow()
            .current_pending_text_track_load_task(task_id)?;
        Some((
            RendererPageTextTrackLoadOwner::new(root_document, target),
            kind,
        ))
    }

    /// Apply one authorized text-track load step without completing its HTML
    /// task.
    ///
    /// `Start` only performs the stable-state/fetch-start section. Terminal
    /// kinds may dispatch `load`/`error`. The selected Page-task dispatcher
    /// distinguishes those effects and owns the later checkpoint and callback
    /// reconciliation.
    pub(crate) fn apply_current_text_track_load_body(
        &mut self,
        authorization: AuthorizedCurrentPageTextTrackLoad,
    ) -> Result<bool> {
        let task = authorization.into_task();
        let owner = task.owner();
        self.with_default_context_scope(|scope, host_ptr| {
            unsafe { &mut *host_ptr }
                .apply_authorized_text_track_load_task(
                    scope,
                    host_ptr,
                    task.task_id(),
                    owner.target(),
                    task.kind(),
                )
                .ok_or_else(|| anyhow!("authorized text-track load task lost its exact payload"))
        })
    }

    /// Cancel an exact stale Host payload without creating a helper-local
    /// checkpoint.
    ///
    /// A displaced media readiness gate can publish a later media-element
    /// task. That publication is body work, not a reason to flush unrelated
    /// runtime scripts here. The returned fact lets the selected dispatcher
    /// retain one ordinary task-end checkpoint only when a payload was
    /// actually consumed.
    pub(crate) fn discard_stale_text_track_load_task_body(
        &mut self,
        task_id: RendererPageTextTrackLoadTaskId,
    ) -> Result<bool> {
        let discarded = self
            ._context_host
            .borrow_mut()
            .discard_pending_text_track_load_task(task_id);
        let Some(discarded) = discarded else {
            return Ok(false);
        };
        let Some(followup) = discarded.into_media_canplay_followup() else {
            return Ok(true);
        };
        self.with_default_context_scope(|scope, host_ptr| {
            crate::native_bridge::element::queue_media_canplay_after_text_tracks(
                scope,
                host_ptr,
                Some(followup),
            );
            Ok(())
        })?;
        Ok(true)
    }
}
