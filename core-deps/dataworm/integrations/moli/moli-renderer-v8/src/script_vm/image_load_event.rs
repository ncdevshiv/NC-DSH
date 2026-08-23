use anyhow::{Result, anyhow};

use super::ScriptVm;
use crate::{
    page_task_queue::{
        PageImageLoadEventTargetEffect, RendererPageImageLoadEventKind,
        RendererPageImageLoadEventOwner, RendererPageImageLoadEventTaskId,
    },
    runtime::AuthorizedCurrentPageImageLoadEvent,
};

impl ScriptVm {
    pub(crate) fn current_pending_image_load_event_owner(
        &self,
        task_id: RendererPageImageLoadEventTaskId,
        root_document: crate::runtime::RendererDocumentToken,
    ) -> Option<(
        RendererPageImageLoadEventOwner,
        RendererPageImageLoadEventKind,
    )> {
        let (target, kind) = self
            ._context_host
            .borrow()
            .current_pending_image_load_event_task(task_id)?;
        Some((
            RendererPageImageLoadEventOwner::new(root_document, target),
            kind,
        ))
    }

    /// Apply one already-authorized image terminal task body.
    ///
    /// Network state has already reached a terminal result before this task is
    /// published. This body consumes the exact request sequence, dispatches
    /// its load/error event when appropriate, processes decode settlements,
    /// and releases the request's load-delay binding. The selected
    /// DOM-manipulation dispatcher owns the later task checkpoint, child
    /// synchronization, and runtime-script follow-up.
    pub(crate) fn apply_current_image_load_event_body(
        &mut self,
        authorization: AuthorizedCurrentPageImageLoadEvent,
    ) -> Result<PageImageLoadEventTargetEffect> {
        let task = authorization.into_task();
        let owner = task.owner();
        let task_id = task.task_id();
        let kind = task.kind();
        let target_effect = self
            .with_default_context_scope(|scope, host_ptr| {
                Ok(unsafe { &mut *host_ptr }.apply_authorized_image_load_event(
                    scope,
                    host_ptr,
                    task_id,
                    owner.target(),
                    kind,
                ))
            })?
            .ok_or_else(|| anyhow!("authorized image load event lost its exact pending payload"))?;
        Ok(target_effect)
    }

    /// Retire an exact stale Host payload without manufacturing a checkpoint
    /// for a task that did not own anything.
    ///
    /// Successfully settling the payload can make `image.decode()` promises
    /// ready. Those settlements are performed without a helper-local
    /// checkpoint and reported to the selected-task dispatcher, which then
    /// completes the task once. A missing/already-retired payload never enters
    /// V8.
    pub(crate) fn discard_stale_image_load_event_task_body(
        &mut self,
        task_id: RendererPageImageLoadEventTaskId,
    ) -> Result<bool> {
        let settled = self
            ._context_host
            .borrow_mut()
            .discard_stale_pending_image_load_event_task(task_id);
        if !settled {
            return Ok(false);
        }
        self.with_default_context_scope(|scope, host_ptr| {
            let _ = unsafe { &mut *host_ptr }.process_pending_image_decode_requests(scope);
            Ok(true)
        })
    }

    /// Apply one ImageLoadEvent domain body in a ScriptVm-only fixture.
    ///
    /// This support hook deliberately stops before Page-task completion:
    /// it performs no microtask checkpoint, child synchronization, or runtime
    /// follow-up. Complete image-event workflows must use
    /// `PageVmTaskExecutorTestHarness`; this hook remains only for tests that
    /// intentionally inspect body ordering against another low-level body,
    /// such as child-document owner retirement.
    #[cfg(test)]
    pub(crate) fn apply_next_image_load_event_body_for_test(&mut self) -> Result<bool> {
        let residence = self
            ._page_task_residence_for_executor_test
            .as_ref()
            .expect("image body fixture must retain its production Page source");
        let source = residence.task_sources();
        let root_document = residence.root_document();
        let Some(task) = source.take_scheduler_task_for_executor_test(|descriptor| {
            matches!(
                descriptor,
                crate::page_task_queue::RendererPageReadyDescriptor::DomManipulation {
                    owner: crate::page_task_queue::RendererPageDomManipulationOwner::ImageLoadEvent(
                        _
                    ),
                    ..
                }
            )
        }) else {
            return Ok(false);
        };
        let crate::page_task_queue::RendererPageSchedulerTask::DomManipulation(
            crate::page_task_queue::RendererPageDomManipulationTask::ImageLoadEvent(task),
        ) = task
        else {
            unreachable!("image descriptor must dequeue its own DOM-manipulation task")
        };
        let owner = task.owner();
        let task_id = task.task_id();
        let kind = task.kind();
        if self.current_pending_image_load_event_owner(task_id, root_document)
            == Some((owner, kind))
        {
            let _ = self.apply_current_image_load_event_body(
                AuthorizedCurrentPageImageLoadEvent::new_for_executor_test(task),
            )?;
        } else {
            assert_eq!(
                owner.root_document(),
                root_document,
                "a ScriptVm body fixture cannot authorize a foreign root Page"
            );
            let _ = self.discard_stale_image_load_event_task_body(task_id)?;
        }
        Ok(true)
    }
}
