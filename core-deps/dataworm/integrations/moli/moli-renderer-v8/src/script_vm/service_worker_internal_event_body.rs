//! Event-dispatch bodies for browser-context ServiceWorker internal tasks.
//!
//! Event dispatch stays body-only. The returned callback fact lets the Page
//! layer distinguish a real callback task from an event pass that found no
//! callback, without storing completion policy in the queued task.

use anyhow::Result;

use super::{ScriptVm, ServiceWorkerInternalBodyEffect};
use crate::types::{ServiceWorkerControllerChangeCompletion, ServiceWorkerLifecycleNotification};

impl ScriptVm {
    pub(crate) fn apply_service_worker_lifecycle_body(
        &mut self,
        notification: ServiceWorkerLifecycleNotification,
    ) -> Result<ServiceWorkerInternalBodyEffect> {
        let callback_effect = self.with_default_context_scope(move |scope, host_ptr| {
            Ok(
                crate::context_bootstrap::dispatch_service_worker_lifecycle_notification(
                    scope,
                    unsafe { &mut *host_ptr },
                    notification,
                ),
            )
        })?;
        Ok(
            ServiceWorkerInternalBodyEffect::EventDispatchPassCompleted {
                callback_effect: callback_effect.into(),
            },
        )
    }

    pub(crate) fn apply_service_worker_controller_change_body(
        &mut self,
        completion: ServiceWorkerControllerChangeCompletion,
    ) -> Result<ServiceWorkerInternalBodyEffect> {
        let Some(owner) = self
            ._context_host
            .borrow()
            .service_worker_window_client_completion_owner(completion.target)
        else {
            return Ok(ServiceWorkerInternalBodyEffect::ExactTargetUnavailable);
        };

        let callback_effect = self.with_default_context_scope(move |scope, _host_ptr| {
            Ok(
                crate::context_bootstrap::dispatch_service_worker_controller_change(
                    scope,
                    owner.dispatch_scope(),
                ),
            )
        })?;
        Ok(
            ServiceWorkerInternalBodyEffect::EventDispatchPassCompleted {
                callback_effect: callback_effect.into(),
            },
        )
    }
}
