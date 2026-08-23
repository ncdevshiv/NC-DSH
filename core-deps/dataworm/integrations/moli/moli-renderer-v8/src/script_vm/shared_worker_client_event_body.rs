//! Exact-target SharedWorker client-event body execution.
//!
//! SharedWorker client events have a smaller domain than DedicatedWorker
//! events: `Closed` retires the renderer endpoint without dispatching JS,
//! while `Error` may retain or retire the endpoint and may dispatch an error
//! listener. This component performs only those state changes and callback
//! bodies. The selected Page-task dispatcher owns the ordinary task-end
//! checkpoint and any callback reconciliation.

use anyhow::Result;

use super::ScriptVm;
use crate::{
    runtime::AuthorizedCurrentPageSharedWorkerClientEvent,
    shared_worker_runtime::{SharedWorkerClientEndpointDisposition, SharedWorkerClientEvent},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SharedWorkerErrorDispatchEffect {
    /// At least one matching error listener ran in the exact wrapper realm.
    CallbackDispatched,
    /// The exact wrapper realm was entered, but no error listener matched.
    CurrentTargetHadNoCallback,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SharedWorkerClientEventBodyEffect {
    /// `Closed` retired the exact renderer-local endpoint. It is an internal
    /// state transition, not a user-visible `SharedWorker` callback.
    EndpointClosed,
    /// One error was applied to the exact endpoint. Endpoint lifetime and JS
    /// dispatch are orthogonal and therefore remain separately typed.
    Error {
        endpoint_disposition: SharedWorkerClientEndpointDisposition,
        dispatch_effect: SharedWorkerErrorDispatchEffect,
    },
}

impl ScriptVm {
    pub(crate) fn current_shared_worker_client_event_identity(
        &self,
        client_id: moli_shared_worker::SharedWorkerClientId,
    ) -> Option<crate::native_bridge::WindowExecutionContextIdentity> {
        self._context_host
            .borrow()
            .current_shared_worker_client_event_identity(client_id)
    }

    /// Apply one exact SharedWorker client-event body without completing the
    /// selected HTML task.
    pub(crate) fn apply_current_shared_worker_client_event_body(
        &mut self,
        authorized: AuthorizedCurrentPageSharedWorkerClientEvent,
    ) -> Result<SharedWorkerClientEventBodyEffect> {
        let task = authorized.into_task();
        let owner = task.owner();
        let event = task.into_event();
        let context_host = self._context_host.clone();
        match event {
            SharedWorkerClientEvent::Closed => {
                context_host
                    .borrow_mut()
                    .apply_authorized_shared_worker_client_close(
                        owner.client_id(),
                        owner.execution_context(),
                    );
                Ok(SharedWorkerClientEventBodyEffect::EndpointClosed)
            }
            SharedWorkerClientEvent::Error(error) => {
                self.with_default_context_scope(|scope, _host_ptr| {
                    let endpoint_disposition = error.endpoint_disposition();
                    let (owner_scope, wrapper, applied_endpoint_disposition) = context_host
                        .borrow_mut()
                        .apply_authorized_shared_worker_client_error(
                            scope,
                            owner.client_id(),
                            owner.execution_context(),
                            endpoint_disposition,
                        )
                        .into_parts();
                    let worker_context = wrapper.get_creation_context(scope).expect(
                        "an exact-current SharedWorker endpoint must retain its wrapper realm",
                    );
                    let scope = &mut v8::ContextScope::new(scope, worker_context);
                    let previous_owner_context = owner_scope.enter(scope);
                    let callback_dispatched =
                        crate::context_bootstrap::dispatch_shared_worker_client_error(
                            scope, wrapper, &error,
                        );
                    if callback_dispatched {
                        owner_scope.defer_restore(scope, previous_owner_context);
                    } else {
                        owner_scope.restore(scope, previous_owner_context);
                    }
                    Ok(SharedWorkerClientEventBodyEffect::Error {
                        endpoint_disposition: applied_endpoint_disposition,
                        dispatch_effect: if callback_dispatched {
                            SharedWorkerErrorDispatchEffect::CallbackDispatched
                        } else {
                            SharedWorkerErrorDispatchEffect::CurrentTargetHadNoCallback
                        },
                    })
                })
            }
        }
    }
}
