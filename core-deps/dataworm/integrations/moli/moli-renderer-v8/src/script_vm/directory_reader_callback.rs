use anyhow::{Result, anyhow};

use super::ScriptVm;
use crate::{
    page_task_queue::{
        PageFileReadingTargetEffect, RendererPageFileReadingOwner, RendererPageFileReadingTaskId,
        RendererPageFileReadingTaskKind,
    },
    runtime::{AuthorizedCurrentPageFileReadingTask, RendererDocumentToken},
};

impl ScriptVm {
    pub(crate) fn current_pending_directory_reader_callback_owner(
        &self,
        task_id: RendererPageFileReadingTaskId,
        root_document: RendererDocumentToken,
    ) -> Option<(
        RendererPageFileReadingOwner,
        RendererPageFileReadingTaskKind,
    )> {
        let (target, kind) = self
            ._context_host
            .borrow()
            .current_pending_directory_reader_callback_task(task_id)?;
        Some((
            RendererPageFileReadingOwner::new(root_document, target),
            kind,
        ))
    }

    /// Apply one already-authorized directory-reader callback body.
    ///
    /// FileReading selection owns the task checkpoint and runtime follow-up;
    /// this method owns only the exact reader transition and Web IDL callback.
    pub(crate) fn apply_current_directory_reader_callback_body(
        &mut self,
        authorization: AuthorizedCurrentPageFileReadingTask,
    ) -> Result<PageFileReadingTargetEffect> {
        let task = authorization.into_task();
        let owner = task.owner();
        let task_id = task.task_id();
        let kind = task.kind();
        let Some(callback) = self
            ._context_host
            .borrow_mut()
            .take_pending_directory_reader_callback_for_exact_target(task_id, owner.target(), kind)
        else {
            return Err(anyhow!(
                "authorized FileSystemDirectoryReader.readEntries task lost its exact pending request"
            ));
        };
        self.with_default_context_scope(|scope, host_ptr| {
            Ok(
                unsafe { &mut *host_ptr }.dispatch_authorized_directory_reader_callback(
                    scope,
                    host_ptr,
                    owner.target(),
                    callback,
                ),
            )
        })
    }

    pub(crate) fn discard_stale_directory_reader_callback_task(
        &mut self,
        task_id: RendererPageFileReadingTaskId,
    ) -> bool {
        self.with_default_context_scope(|scope, host_ptr| {
            Ok::<_, anyhow::Error>(
                unsafe { &mut *host_ptr }
                    .discard_pending_directory_reader_callback_task(scope, task_id),
            )
        })
        .unwrap_or(false)
    }
}
