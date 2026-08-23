use anyhow::{Result, anyhow};

use super::ScriptVm;
use crate::{
    page_task_queue::{
        PageUserInteractionBodyEffect, RendererPageUserInteractionOwner,
        RendererPageUserInteractionTaskId, RendererPageUserInteractionTaskKind,
    },
    runtime::AuthorizedCurrentPageUserInteractionTask,
};

impl ScriptVm {
    pub(crate) fn current_pending_user_interaction_owner(
        &self,
        task_id: RendererPageUserInteractionTaskId,
        root_document: crate::runtime::RendererDocumentToken,
    ) -> Option<(
        RendererPageUserInteractionOwner,
        RendererPageUserInteractionTaskKind,
    )> {
        let (target, kind) = self
            ._context_host
            .borrow()
            .current_pending_user_interaction_task(task_id)?;
        Some((
            RendererPageUserInteractionOwner::new(root_document, target),
            kind,
        ))
    }

    /// Apply one already-authorized user-interaction task body.
    ///
    /// The selected Page-task dispatcher owns the task checkpoint,
    /// child-record synchronization, and runtime/style follow-up. Keeping this
    /// method body-only prevents the V8 context helper from introducing an
    /// intermediate checkpoint before the scheduler task has completed.
    pub(crate) fn apply_current_user_interaction_task_body(
        &mut self,
        authorization: AuthorizedCurrentPageUserInteractionTask,
    ) -> Result<PageUserInteractionBodyEffect> {
        let task = authorization.into_task();
        let owner = task.owner();
        let task_id = task.task_id();
        let kind = task.kind();
        let Some(payload) = self
            ._context_host
            .borrow_mut()
            .take_pending_user_interaction_task_for_exact_target(task_id, owner.target(), kind)
        else {
            return Err(anyhow!(
                "authorized user-interaction task lost its exact pending payload"
            ));
        };
        self.with_default_context_scope(|scope, host_ptr| {
            unsafe { &mut *host_ptr }.dispatch_authorized_user_interaction_task(
                scope,
                host_ptr,
                owner.target(),
                kind,
                payload,
            )
        })
    }

    pub(crate) fn discard_stale_user_interaction_task(
        &mut self,
        task_id: RendererPageUserInteractionTaskId,
    ) -> bool {
        self._context_host
            .borrow_mut()
            .discard_pending_user_interaction_task(task_id)
    }
}
