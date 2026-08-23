use anyhow::Result;

use super::ScriptVm;
use crate::page_task_queue::RendererPageHashChangeDeliveryOwner;

impl ScriptVm {
    /// Apply one `hashchange` only after the Page arbiter has matched the root
    /// PageVm namespace and exact recipient LocalDOMWindow.
    ///
    /// This method dispatches only the event body. The selected Page-task
    /// dispatcher owns the task checkpoint, child-record synchronization, and
    /// runtime/style follow-up.
    pub(crate) fn apply_current_hash_change_delivery_body(
        &mut self,
        authorization: crate::runtime::AuthorizedCurrentPageHashChangeDelivery,
    ) -> Result<bool> {
        let task = authorization.into_task();
        let (owner, data) = task.into_parts();
        self.with_default_context_scope(|scope, host_ptr| {
            Ok(
                unsafe { &mut *host_ptr }.dispatch_authorized_hash_change_delivery(
                    scope,
                    host_ptr,
                    owner.target(),
                    &data,
                ),
            )
        })
    }

    pub(crate) fn current_hash_change_delivery_owner(
        &self,
        expected: RendererPageHashChangeDeliveryOwner,
        root_document: crate::runtime::RendererDocumentToken,
    ) -> Option<RendererPageHashChangeDeliveryOwner> {
        self.current_window_task_target(expected.target())
            .map(|target| RendererPageHashChangeDeliveryOwner::new(root_document, target))
    }
}
