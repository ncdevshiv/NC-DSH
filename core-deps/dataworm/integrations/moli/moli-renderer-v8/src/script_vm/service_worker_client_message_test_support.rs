//! Exact ServiceWorker client-message inputs for PageVm responsibility tests.
//!
//! Production receives both values from the browser-context ServiceWorker
//! runtime. Tests build them from the currently registered Window client so a
//! fixture cannot accidentally substitute an arbitrary client id, document
//! epoch, transport generation, or invalid structured-clone payload.
//!
//! This module only constructs exact fixture inputs. It must not claim a Page
//! task, authorize an owner, execute a message body, or submit task completion;
//! complete behavior tests use the production selected-task dispatcher.

use anyhow::{Result, anyhow};

use super::ScriptVm;

impl ScriptVm {
    pub(crate) fn register_service_worker_child_client_for_test(
        &mut self,
        child_handle: crate::document_runtime::DomHandle,
    ) -> Result<crate::service_worker_runtime::ServiceWorkerClientId> {
        self._context_host
            .borrow_mut()
            .register_or_update_service_worker_child_client(child_handle)
            .ok_or_else(|| anyhow!("exact child ServiceWorker client could not be registered"))
    }

    pub(crate) fn service_worker_client_message_payload_for_test(
        &mut self,
        value: &str,
    ) -> Result<crate::structured_clone::V8StructuredClonePayload> {
        self.with_default_context_scope_and_checkpoint_for_test(|scope, _host_ptr| {
            let value = v8::String::new(scope, value)
                .ok_or_else(|| anyhow!("failed to allocate ServiceWorker test message"))?;
            crate::context_bootstrap::structured_serialize_value(scope, value.into())
                .ok_or_else(|| anyhow!("failed to serialize ServiceWorker test message"))
        })
    }

    pub(crate) fn service_worker_client_message_target_for_test(
        &self,
        dispatch_scope: crate::native_bridge::OwnerDispatchScope,
    ) -> Result<crate::types::ServiceWorkerWindowClientTarget> {
        self._context_host
            .borrow()
            .service_worker_window_client_target_for_test(dispatch_scope)
            .ok_or_else(|| anyhow!("exact ServiceWorker Window client target disappeared"))
    }
}
