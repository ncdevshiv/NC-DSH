use anyhow::{Result, anyhow};

use super::ScriptVm;
use crate::{
    page_task_queue::{
        PageFileEntryFileCallbackTargetEffect, RendererPageFileEntryFileCallbackOwner,
        RendererPageFileEntryFileCallbackTaskId, RendererPageFileEntryFileCallbackTaskKind,
    },
    runtime::AuthorizedCurrentPageFileEntryFileCallback,
};

impl ScriptVm {
    pub(crate) fn current_pending_file_entry_file_callback_owner(
        &self,
        task_id: RendererPageFileEntryFileCallbackTaskId,
        root_document: crate::runtime::RendererDocumentToken,
    ) -> Option<(
        RendererPageFileEntryFileCallbackOwner,
        RendererPageFileEntryFileCallbackTaskKind,
    )> {
        let (target, kind) = self
            ._context_host
            .borrow()
            .current_pending_file_entry_file_callback_task(task_id)?;
        Some((
            RendererPageFileEntryFileCallbackOwner::new(root_document, target),
            kind,
        ))
    }

    /// Apply one already-authorized FileEntry callback body.
    ///
    /// The selected DOM-manipulation dispatcher owns the task checkpoint,
    /// child-record synchronization, and runtime/style follow-up.
    pub(crate) fn apply_current_file_entry_file_callback_body(
        &mut self,
        authorization: AuthorizedCurrentPageFileEntryFileCallback,
    ) -> Result<PageFileEntryFileCallbackTargetEffect> {
        let task = authorization.into_task();
        let owner = task.owner();
        let task_id = task.task_id();
        let kind = task.kind();
        let Some(callback) = self
            ._context_host
            .borrow_mut()
            .take_pending_file_entry_file_callback_for_exact_target(task_id, owner.target(), kind)
        else {
            return Err(anyhow!(
                "authorized FileSystemFileEntry.file task lost its exact pending callback"
            ));
        };
        self.with_default_context_scope(|scope, host_ptr| {
            Ok(
                unsafe { &mut *host_ptr }.dispatch_authorized_file_entry_file_callback(
                    scope,
                    host_ptr,
                    owner.target(),
                    callback,
                ),
            )
        })
    }

    pub(crate) fn discard_stale_file_entry_file_callback_task(
        &mut self,
        task_id: RendererPageFileEntryFileCallbackTaskId,
    ) -> bool {
        self._context_host
            .borrow_mut()
            .discard_pending_file_entry_file_callback_task(task_id)
    }
}
