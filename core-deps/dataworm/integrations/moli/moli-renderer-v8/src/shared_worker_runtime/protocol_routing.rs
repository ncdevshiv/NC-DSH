use moli_shared_worker::SharedWorkerInstanceId;

use crate::runtime::{RendererRuntimeInspectorMessage, RendererRuntimeInspectorResponseSender};

use super::service::SharedWorkerRuntimeService;

impl SharedWorkerRuntimeService {
    pub(crate) async fn dispatch_runtime_protocol_message(
        &self,
        instance_id: SharedWorkerInstanceId,
        inspector_session_id: Option<String>,
        raw_json: String,
    ) -> Result<Vec<RendererRuntimeInspectorMessage>, String> {
        let Some(host) = self.running_host_for_instance(instance_id) else {
            return Err("SharedWorkerRuntimeUnavailable".to_owned());
        };
        host.dispatch_runtime_protocol_message(inspector_session_id, raw_json)
            .await
    }

    pub(crate) async fn dispatch_runtime_protocol_message_with_deferred_response(
        &self,
        instance_id: SharedWorkerInstanceId,
        inspector_session_id: Option<String>,
        raw_json: String,
        deferred_response: RendererRuntimeInspectorResponseSender,
    ) -> Result<Vec<RendererRuntimeInspectorMessage>, String> {
        let Some(host) = self.running_host_for_instance(instance_id) else {
            return Err("SharedWorkerRuntimeUnavailable".to_owned());
        };
        host.dispatch_runtime_protocol_message_with_deferred_response(
            inspector_session_id,
            raw_json,
            deferred_response,
        )
        .await
    }

    pub(crate) fn detach_runtime_inspector_session(
        &self,
        instance_id: SharedWorkerInstanceId,
        inspector_session_id: Option<String>,
    ) -> bool {
        let Some(host) = self.running_host_for_instance(instance_id) else {
            return false;
        };
        host.detach_runtime_inspector_session(inspector_session_id)
    }
}
