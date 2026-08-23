use crate::{
    RendererSyntheticResponseBody,
    shared_worker_runtime::{SharedWorkerLaunchParams, SharedWorkerRuntimeOwnerWakeSender},
    worker::{WorkerPendingFetchContinue, WorkerPendingXhrContinue},
};
use moli_shared_worker::{
    SharedWorkerClientId, SharedWorkerClientOwnerId, SharedWorkerDescriptor, SharedWorkerInstanceId,
};

use super::RendererBrowserContextRuntime;
use crate::runtime::{
    RendererOwnerLocalHostId, RendererRuntimeInspectorMessage,
    RendererRuntimeInspectorResponseSender,
};

impl RendererBrowserContextRuntime {
    pub(crate) fn add_shared_worker_owner_wake_sender(
        &self,
        sender: SharedWorkerRuntimeOwnerWakeSender,
    ) {
        self.inner
            .shared_worker_runtime
            .add_owner_wake_sender(sender);
    }

    pub(crate) fn set_shared_worker_owner_local_host_id(
        &self,
        owner_local_host_id: RendererOwnerLocalHostId,
    ) {
        self.inner
            .shared_worker_runtime
            .set_owner_local_host_id(owner_local_host_id);
    }

    pub(crate) fn connect_shared_worker(
        &self,
        descriptor: SharedWorkerDescriptor,
        params: SharedWorkerLaunchParams,
    ) -> SharedWorkerClientId {
        self.inner.shared_worker_runtime.connect(descriptor, params)
    }

    pub(crate) fn next_shared_worker_client_owner_id(&self) -> SharedWorkerClientOwnerId {
        self.inner.shared_worker_runtime.next_client_owner_id()
    }

    pub(crate) fn drain_shared_worker_service_lane(&self) -> usize {
        self.inner.shared_worker_runtime.drain_service_lane()
    }

    pub fn close_shared_worker_for_target_close(
        &self,
        instance_id: SharedWorkerInstanceId,
    ) -> bool {
        self.inner
            .shared_worker_runtime
            .close_instance_for_devtools_target_close(instance_id)
    }

    pub(crate) fn remove_shared_worker_client(&self, client_id: SharedWorkerClientId) {
        self.inner.shared_worker_runtime.remove_client(client_id);
    }

    pub(crate) fn continue_shared_worker_fetch(
        &self,
        instance_id: SharedWorkerInstanceId,
        request: WorkerPendingFetchContinue,
    ) -> bool {
        self.inner
            .shared_worker_runtime
            .continue_pending_fetch(instance_id, request)
    }

    pub(crate) fn continue_shared_worker_xhr(
        &self,
        instance_id: SharedWorkerInstanceId,
        request: WorkerPendingXhrContinue,
    ) -> bool {
        self.inner
            .shared_worker_runtime
            .continue_pending_xhr(instance_id, request)
    }

    pub(crate) fn continue_shared_worker_csp_report(
        &self,
        instance_id: SharedWorkerInstanceId,
        request: WorkerPendingFetchContinue,
    ) -> bool {
        self.inner
            .shared_worker_runtime
            .continue_pending_csp_report(instance_id, request)
    }

    pub(crate) fn continue_shared_worker_fetch_response(
        &self,
        instance_id: SharedWorkerInstanceId,
        request: WorkerPendingFetchContinue,
        response_code: Option<u16>,
        response_headers: Option<Vec<(String, String)>>,
    ) -> bool {
        self.inner
            .shared_worker_runtime
            .continue_pending_fetch_response(instance_id, request, response_code, response_headers)
    }

    pub(crate) fn continue_shared_worker_xhr_response(
        &self,
        instance_id: SharedWorkerInstanceId,
        request: WorkerPendingXhrContinue,
        response_code: Option<u16>,
        response_headers: Option<Vec<(String, String)>>,
    ) -> bool {
        self.inner
            .shared_worker_runtime
            .continue_pending_xhr_response(instance_id, request, response_code, response_headers)
    }

    pub(crate) fn fail_shared_worker_fetch(
        &self,
        instance_id: SharedWorkerInstanceId,
        request: WorkerPendingFetchContinue,
        error_text: String,
    ) -> bool {
        self.inner
            .shared_worker_runtime
            .fail_pending_fetch(instance_id, request, error_text)
    }

    pub(crate) fn fail_shared_worker_xhr(
        &self,
        instance_id: SharedWorkerInstanceId,
        request: WorkerPendingXhrContinue,
        error_text: String,
    ) -> bool {
        self.inner
            .shared_worker_runtime
            .fail_pending_xhr(instance_id, request, error_text)
    }

    pub(crate) fn fail_shared_worker_csp_report(
        &self,
        instance_id: SharedWorkerInstanceId,
        request: WorkerPendingFetchContinue,
        error_text: String,
    ) -> bool {
        self.inner
            .shared_worker_runtime
            .fail_pending_csp_report(instance_id, request, error_text)
    }

    pub(crate) fn fail_shared_worker_fetch_auth(
        &self,
        instance_id: SharedWorkerInstanceId,
        request: WorkerPendingFetchContinue,
        error_text: String,
    ) -> bool {
        self.inner
            .shared_worker_runtime
            .fail_pending_fetch_auth(instance_id, request, error_text)
    }

    pub(crate) fn fail_shared_worker_xhr_auth(
        &self,
        instance_id: SharedWorkerInstanceId,
        request: WorkerPendingXhrContinue,
        error_text: String,
    ) -> bool {
        self.inner
            .shared_worker_runtime
            .fail_pending_xhr_auth(instance_id, request, error_text)
    }

    pub(crate) fn fail_shared_worker_fetch_response(
        &self,
        instance_id: SharedWorkerInstanceId,
        request: WorkerPendingFetchContinue,
        error_text: String,
    ) -> bool {
        self.inner
            .shared_worker_runtime
            .fail_pending_fetch_response(instance_id, request, error_text)
    }

    pub(crate) fn fail_shared_worker_xhr_response(
        &self,
        instance_id: SharedWorkerInstanceId,
        request: WorkerPendingXhrContinue,
        error_text: String,
    ) -> bool {
        self.inner
            .shared_worker_runtime
            .fail_pending_xhr_response(instance_id, request, error_text)
    }

    pub(crate) fn fulfill_shared_worker_fetch(
        &self,
        instance_id: SharedWorkerInstanceId,
        request: WorkerPendingFetchContinue,
        response_code: u16,
        response_headers: Vec<(String, String)>,
        response_body: RendererSyntheticResponseBody,
    ) -> bool {
        self.inner.shared_worker_runtime.fulfill_pending_fetch(
            instance_id,
            request,
            response_code,
            response_headers,
            response_body,
        )
    }

    pub(crate) fn fulfill_shared_worker_xhr(
        &self,
        instance_id: SharedWorkerInstanceId,
        request: WorkerPendingXhrContinue,
        response_code: u16,
        response_headers: Vec<(String, String)>,
        response_body: RendererSyntheticResponseBody,
    ) -> bool {
        self.inner.shared_worker_runtime.fulfill_pending_xhr(
            instance_id,
            request,
            response_code,
            response_headers,
            response_body,
        )
    }

    pub(crate) fn fulfill_shared_worker_csp_report(
        &self,
        instance_id: SharedWorkerInstanceId,
        request: WorkerPendingFetchContinue,
        response_code: u16,
        response_headers: Vec<(String, String)>,
        response_body: RendererSyntheticResponseBody,
    ) -> bool {
        self.inner.shared_worker_runtime.fulfill_pending_csp_report(
            instance_id,
            request,
            response_code,
            response_headers,
            response_body,
        )
    }

    pub(crate) fn fulfill_shared_worker_fetch_response(
        &self,
        instance_id: SharedWorkerInstanceId,
        request: WorkerPendingFetchContinue,
        response_code: u16,
        response_headers: Vec<(String, String)>,
        response_body: RendererSyntheticResponseBody,
    ) -> bool {
        self.inner
            .shared_worker_runtime
            .fulfill_pending_fetch_response(
                instance_id,
                request,
                response_code,
                response_headers,
                response_body,
            )
    }

    pub(crate) fn fulfill_shared_worker_xhr_response(
        &self,
        instance_id: SharedWorkerInstanceId,
        request: WorkerPendingXhrContinue,
        response_code: u16,
        response_headers: Vec<(String, String)>,
        response_body: RendererSyntheticResponseBody,
    ) -> bool {
        self.inner
            .shared_worker_runtime
            .fulfill_pending_xhr_response(
                instance_id,
                request,
                response_code,
                response_headers,
                response_body,
            )
    }

    pub async fn dispatch_shared_worker_runtime_protocol_message(
        &self,
        instance_id: SharedWorkerInstanceId,
        inspector_session_id: Option<String>,
        raw_json: String,
    ) -> Result<Vec<RendererRuntimeInspectorMessage>, String> {
        self.inner
            .shared_worker_runtime
            .dispatch_runtime_protocol_message(instance_id, inspector_session_id, raw_json)
            .await
    }

    pub async fn dispatch_shared_worker_runtime_protocol_message_with_deferred_response(
        &self,
        instance_id: SharedWorkerInstanceId,
        inspector_session_id: Option<String>,
        raw_json: String,
        deferred_response: RendererRuntimeInspectorResponseSender,
    ) -> Result<Vec<RendererRuntimeInspectorMessage>, String> {
        self.inner
            .shared_worker_runtime
            .dispatch_runtime_protocol_message_with_deferred_response(
                instance_id,
                inspector_session_id,
                raw_json,
                deferred_response,
            )
            .await
    }

    pub fn detach_shared_worker_runtime_inspector_session(
        &self,
        instance_id: SharedWorkerInstanceId,
        inspector_session_id: Option<String>,
    ) -> bool {
        self.inner
            .shared_worker_runtime
            .detach_runtime_inspector_session(instance_id, inspector_session_id)
    }
}
