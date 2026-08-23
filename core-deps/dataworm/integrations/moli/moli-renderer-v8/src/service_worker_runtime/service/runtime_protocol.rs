use crate::runtime::{RendererRuntimeInspectorMessage, RendererRuntimeInspectorResponseSender};

use super::super::ids::ServiceWorkerVersionId;
use super::ServiceWorkerRuntimeService;

impl ServiceWorkerRuntimeService {
    pub(crate) async fn dispatch_runtime_protocol_message(
        &self,
        version_id: ServiceWorkerVersionId,
        inspector_session_id: Option<String>,
        raw_json: String,
    ) -> Result<Vec<RendererRuntimeInspectorMessage>, String> {
        let Some(host) = self.running_host_for_version(version_id) else {
            return Err("ServiceWorkerRuntimeUnavailable".to_owned());
        };
        host.dispatch_worker_runtime_protocol_message_without_deferred_response(
            inspector_session_id,
            raw_json,
        )
        .await
    }

    pub(crate) async fn dispatch_runtime_protocol_message_with_deferred_response(
        &self,
        version_id: ServiceWorkerVersionId,
        inspector_session_id: Option<String>,
        raw_json: String,
        deferred_response: RendererRuntimeInspectorResponseSender,
    ) -> Result<Vec<RendererRuntimeInspectorMessage>, String> {
        let Some(host) = self.running_host_for_version(version_id) else {
            return Err("ServiceWorkerRuntimeUnavailable".to_owned());
        };
        host.dispatch_worker_runtime_protocol_message_with_deferred_response(
            inspector_session_id,
            raw_json,
            deferred_response,
        )
        .await
    }

    pub(crate) fn detach_runtime_inspector_session(
        &self,
        version_id: ServiceWorkerVersionId,
        inspector_session_id: Option<String>,
    ) -> bool {
        let Some(host) = self.running_host_for_version(version_id) else {
            return false;
        };
        host.detach_worker_runtime_inspector_session(inspector_session_id)
    }
}
