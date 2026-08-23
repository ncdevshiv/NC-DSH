use anyhow::{Result, anyhow};

use super::ScriptVm;
use crate::{
    page_task_queue::{
        PageMiscPlatformApiTargetEffect, RendererPageMiscPlatformApiOwner,
        RendererPageMiscPlatformApiTaskId, RendererPageMiscPlatformApiTaskKind,
    },
    runtime::{AuthorizedCurrentPageMiscPlatformApiTask, RendererDocumentToken},
};

impl ScriptVm {
    pub(crate) fn current_pending_misc_platform_api_owner(
        &self,
        task_id: RendererPageMiscPlatformApiTaskId,
        root_document: RendererDocumentToken,
    ) -> Option<(
        RendererPageMiscPlatformApiOwner,
        RendererPageMiscPlatformApiTaskKind,
    )> {
        let (target, kind) = self
            ._context_host
            .borrow()
            .current_pending_misc_platform_api_task(task_id)?;
        Some((
            RendererPageMiscPlatformApiOwner::new(root_document, target),
            kind,
        ))
    }

    /// Execute one already-authorized miscellaneous-platform callback body.
    ///
    /// The selected Page-task dispatcher, not this body, owns checkpoint and
    /// runtime follow-up.
    pub(crate) fn apply_current_misc_platform_api_task_body(
        &mut self,
        authorization: AuthorizedCurrentPageMiscPlatformApiTask,
    ) -> Result<PageMiscPlatformApiTargetEffect> {
        let task = authorization.into_task();
        let owner = task.owner();
        let task_id = task.task_id();
        let kind = task.kind();
        let Some(callback) = self
            ._context_host
            .borrow_mut()
            .take_pending_misc_platform_api_task_for_exact_target(task_id, owner.target(), kind)
        else {
            return Err(anyhow!(
                "authorized miscellaneous-platform API task lost its exact callback payload"
            ));
        };
        self.with_default_context_scope(|scope, host_ptr| {
            Ok(
                unsafe { &mut *host_ptr }.dispatch_authorized_misc_platform_api_task(
                    scope,
                    host_ptr,
                    owner.target(),
                    callback,
                ),
            )
        })
    }

    pub(crate) fn discard_stale_misc_platform_api_task(
        &mut self,
        task_id: RendererPageMiscPlatformApiTaskId,
    ) -> bool {
        self._context_host
            .borrow_mut()
            .discard_pending_misc_platform_api_task(task_id)
    }
}
