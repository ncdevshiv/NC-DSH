use crate::{
    context_bootstrap::{
        DirectoryReaderCallbackAdmission, DirectoryReaderCallbackTask,
        DirectoryReaderCallbackTaskEffect,
    },
    page_task_queue::{
        PageFileReadingTargetEffect, RendererPageFileReadingTaskId, RendererPageFileReadingTaskKind,
    },
};
use moli_webidl_callback::WebIdlCallbackFunction;

use super::window_document_tasks::{ExactWindowDocumentTaskLedger, PendingExactWindowDocumentTask};
use super::{JsContextHost, WindowDocumentTaskTarget};

pub(super) type DirectoryReaderCallbackState = ExactWindowDocumentTaskLedger<
    RendererPageFileReadingTaskId,
    RendererPageFileReadingTaskKind,
    DirectoryReaderCallbackTask,
>;

impl JsContextHost {
    /// Admit one `readEntries()` operation on the exact calling
    /// Window/Document and publish its immutable envelope to FileReading.
    pub(crate) fn queue_directory_reader_callback_task<'s>(
        &mut self,
        scope: &mut v8::PinScope<'s, '_>,
        reader: v8::Local<'s, v8::Object>,
        success_callback: WebIdlCallbackFunction,
        error_callback: Option<WebIdlCallbackFunction>,
    ) -> bool {
        let Some(target) = self.current_window_document_task_target(scope) else {
            return false;
        };
        let task_id = self
            .directory_reader_callbacks
            .allocate_task_id(RendererPageFileReadingTaskId::from_raw);
        let admission = DirectoryReaderCallbackTask::admit(
            scope,
            self,
            reader,
            success_callback,
            error_callback,
            task_id,
        );
        let DirectoryReaderCallbackAdmission::Task { kind, task } = admission else {
            return true;
        };
        self.directory_reader_callbacks
            .push(PendingExactWindowDocumentTask::new(
                task_id, target, kind, task,
            ));

        if self
            .page_file_reading_sender()
            .send(target, task_id, kind)
            .is_ok()
        {
            return true;
        }

        if let Some(pending) = self
            .directory_reader_callbacks
            .remove_exact(task_id, target, kind)
        {
            pending.payload().rollback_admission(scope);
        }
        tracing::debug!(
            ?target,
            ?task_id,
            ?kind,
            "retired directory-reader callback after FileReading route closure"
        );
        false
    }

    pub(crate) fn current_pending_directory_reader_callback_task(
        &self,
        task_id: RendererPageFileReadingTaskId,
    ) -> Option<(WindowDocumentTaskTarget, RendererPageFileReadingTaskKind)> {
        let pending = self.directory_reader_callbacks.pending(task_id)?;
        let current_target = self.current_window_document_task_target_for_dispatch_scope(
            pending.target().dispatch_scope(),
        )?;
        Some((current_target, pending.kind()))
    }

    pub(crate) fn take_pending_directory_reader_callback_for_exact_target(
        &mut self,
        task_id: RendererPageFileReadingTaskId,
        target: WindowDocumentTaskTarget,
        kind: RendererPageFileReadingTaskKind,
    ) -> Option<DirectoryReaderCallbackTask> {
        self.directory_reader_callbacks
            .remove_exact(task_id, target, kind)
            .map(PendingExactWindowDocumentTask::into_payload)
    }

    /// Discard one stale scheduler envelope and roll back only the exact
    /// active reader request installed by that payload.
    pub(crate) fn discard_pending_directory_reader_callback_task(
        &mut self,
        scope: &mut v8::PinScope<'_, '_>,
        task_id: RendererPageFileReadingTaskId,
    ) -> bool {
        let Some(pending) = self.directory_reader_callbacks.remove(task_id) else {
            return false;
        };
        pending.payload().rollback_admission(scope);
        true
    }

    pub(crate) fn dispatch_authorized_directory_reader_callback(
        &mut self,
        scope: &mut v8::PinScope<'_, '_>,
        host_ptr: *mut JsContextHost,
        target: WindowDocumentTaskTarget,
        task: DirectoryReaderCallbackTask,
    ) -> PageFileReadingTargetEffect {
        let Some(resolved) = self.resolve_authorized_window_document_task_context(scope, target)
        else {
            return PageFileReadingTargetEffect::CurrentOwnerCallbackRetired;
        };
        let scope = &mut v8::ContextScope::new(scope, resolved.context);
        let dispatch_scope = target.dispatch_scope();
        let previous_scope = dispatch_scope.enter(scope);
        let effect = match task.invoke(scope, host_ptr) {
            DirectoryReaderCallbackTaskEffect::CallbackInvoked => {
                PageFileReadingTargetEffect::CallbackInvokedForCurrentOwner
            }
            DirectoryReaderCallbackTaskEffect::CallbackNotInvoked => {
                PageFileReadingTargetEffect::CurrentOwnerCallbackRetired
            }
            DirectoryReaderCallbackTaskEffect::StaleReaderRequest => {
                PageFileReadingTargetEffect::DiscardedStaleReaderRequest
            }
        };
        dispatch_scope.restore(scope, previous_scope);
        effect
    }
}
