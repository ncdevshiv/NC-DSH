//! Low-level SharedWorker target access for PageVm responsibility tests.
//!
//! Production receives this capability from the browser-context runtime.
//! Tests use the exact registered endpoint identity rather than guessing a
//! client id or copying the Page source envelope.
//!
//! This support module cannot dequeue, authorize, execute, or complete a
//! SharedWorker task. Full behavior tests use the production selected-task
//! dispatcher.

use anyhow::{Result, ensure};

use super::ScriptVm;

impl ScriptVm {
    pub(crate) fn only_shared_worker_client_event_producer_for_test(
        &self,
    ) -> Result<(
        moli_shared_worker::SharedWorkerClientId,
        crate::page_task_queue::RendererPageSharedWorkerClientEventProducer,
    )> {
        let host = self._context_host.borrow();
        let identities = host.shared_worker_client_identities_for_test();
        ensure!(
            identities.len() == 1,
            "expected exactly one SharedWorker test target, found {}",
            identities.len()
        );
        let (client_id, execution_context) = identities[0];
        let producer = host
            .page_shared_worker_client_event_sender()
            .bind_execution_context(execution_context)
            .bind_client(client_id);
        Ok((client_id, producer))
    }
}
