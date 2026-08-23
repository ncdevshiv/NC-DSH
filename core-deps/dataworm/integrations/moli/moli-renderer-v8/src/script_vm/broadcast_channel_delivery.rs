use anyhow::Result;

use super::ScriptVm;

impl ScriptVm {
    pub(crate) fn broadcast_channel_delivery_owner_is_current(
        &self,
        owner: crate::page_task_queue::RendererPageBroadcastChannelDeliveryOwner,
    ) -> bool {
        self._context_host
            .borrow()
            .window_execution_context_identity_is_current(owner.execution_context())
    }

    /// Applies a page-side BroadcastChannel delivery only after the Page
    /// arbiter has matched its root namespace and exact Window realm.
    ///
    /// This method dispatches only the event body. The selected Page-task
    /// dispatcher owns the single task checkpoint, child-record
    /// synchronization, and runtime/style follow-up.
    pub(crate) fn apply_current_broadcast_channel_delivery_body(
        &mut self,
        authorization: crate::runtime::AuthorizedCurrentBroadcastChannelDelivery,
    ) -> Result<bool> {
        let task = authorization.into_task();
        let expected_context = task.owner().execution_context();
        let channel_id = task.channel_id();
        self.with_default_context_scope(|scope, _host_ptr| {
            Ok(
                crate::context_bootstrap::dispatch_authorized_page_broadcast_channel_event(
                    scope,
                    channel_id,
                    expected_context,
                ),
            )
        })
    }

    pub(crate) fn discard_stale_broadcast_channel_delivery(
        &mut self,
        channel_id: crate::types::BroadcastChannelId,
    ) {
        let mut host = self._context_host.borrow_mut();
        host.forget_broadcast_channel_wrapper(channel_id);
        host.broadcast_channel_registry()
            .close_broadcast_channel(channel_id);
    }
}
