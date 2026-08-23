use anyhow::Result;

use super::ScriptVm;
use crate::page_task_queue::RendererPageStorageEventDeliveryOwner;

impl ScriptVm {
    /// Apply one StorageEvent only after the Page arbiter has matched the root
    /// PageVm namespace and exact recipient LocalDOMWindow.
    ///
    /// This method dispatches only the event body. The selected Page-task
    /// dispatcher owns the task checkpoint, child-record synchronization, and
    /// runtime/style follow-up.
    pub(crate) fn apply_current_storage_event_delivery_body(
        &mut self,
        authorization: crate::runtime::AuthorizedCurrentPageStorageEventDelivery,
    ) -> Result<bool> {
        let task = authorization.into_task();
        let (owner, data) = task.into_parts();
        let target = owner.target();
        self.with_default_context_scope(|scope, host_ptr| {
            Ok(unsafe { &mut *host_ptr }
                .dispatch_authorized_storage_event_delivery(scope, host_ptr, target, &data))
        })
    }

    pub(crate) fn current_storage_event_delivery_owner(
        &self,
        expected: RendererPageStorageEventDeliveryOwner,
        root_document: crate::runtime::RendererDocumentToken,
    ) -> Option<RendererPageStorageEventDeliveryOwner> {
        self.current_window_task_target(expected.target())
            .map(|target| RendererPageStorageEventDeliveryOwner::new(root_document, target))
    }
}
