use anyhow::Result;

use super::ScriptVm;
use crate::{
    context_bootstrap::MessagePortDeliveryRunResult, native_bridge::WindowExecutionContextIdentity,
    runtime::AuthorizedCurrentPageMessagePortDelivery, types::MessagePortId,
};

impl ScriptVm {
    pub(crate) fn current_message_port_execution_context_identity(
        &self,
        port_id: MessagePortId,
    ) -> Option<WindowExecutionContextIdentity> {
        let host = self._context_host.borrow();
        let identity = host.message_port_execution_context_identity(port_id)?;
        host.window_execution_context_identity_is_current(identity)
            .then_some(identity)
    }

    /// Apply one delivery only after the Page arbiter has matched the root
    /// PageVm namespace, port id, and exact Window attachment.
    ///
    /// This method executes only the event body. The selected Page-task
    /// dispatcher owns the task checkpoint, child-record synchronization, and
    /// runtime/style follow-up.
    pub(crate) fn apply_current_message_port_delivery_body(
        &mut self,
        authorization: AuthorizedCurrentPageMessagePortDelivery,
        same_attachment_task_is_ready: bool,
    ) -> Result<MessagePortDeliveryRunResult> {
        let task = authorization.into_task();
        let port_id = task.port_id();
        let expected = task.owner().execution_context();
        self.with_default_context_scope(|scope, _host_ptr| {
            Ok(
                crate::context_bootstrap::dispatch_one_authorized_message_port_event(
                    scope,
                    port_id,
                    expected,
                    same_attachment_task_is_ready,
                ),
            )
        })
    }
}
