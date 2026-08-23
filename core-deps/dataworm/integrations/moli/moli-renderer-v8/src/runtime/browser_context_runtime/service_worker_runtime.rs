use crate::{
    runtime::{RendererRuntimeInspectorMessage, RendererRuntimeInspectorResponseSender},
    service_worker_runtime::{
        ServiceWorkerRegistrationId, ServiceWorkerRuntimeOwnerWakeSender, ServiceWorkerVersionId,
    },
};

use super::RendererBrowserContextRuntime;

impl RendererBrowserContextRuntime {
    pub(crate) fn add_service_worker_owner_wake_sender(
        &self,
        sender: ServiceWorkerRuntimeOwnerWakeSender,
    ) {
        self.inner
            .service_worker_runtime
            .add_owner_wake_sender(sender);
    }

    pub(crate) fn drain_service_worker_service_lane(&self) -> usize {
        self.inner.service_worker_runtime.drain_service_lane()
    }

    pub async fn dispatch_service_worker_runtime_protocol_message(
        &self,
        version_id: u64,
        inspector_session_id: Option<String>,
        raw_json: String,
    ) -> Result<Vec<RendererRuntimeInspectorMessage>, String> {
        self.inner
            .service_worker_runtime
            .dispatch_runtime_protocol_message(
                ServiceWorkerVersionId::from_u64_for_binding(version_id),
                inspector_session_id,
                raw_json,
            )
            .await
    }

    pub async fn dispatch_service_worker_runtime_protocol_message_with_deferred_response(
        &self,
        version_id: u64,
        inspector_session_id: Option<String>,
        raw_json: String,
        deferred_response: RendererRuntimeInspectorResponseSender,
    ) -> Result<Vec<RendererRuntimeInspectorMessage>, String> {
        self.inner
            .service_worker_runtime
            .dispatch_runtime_protocol_message_with_deferred_response(
                ServiceWorkerVersionId::from_u64_for_binding(version_id),
                inspector_session_id,
                raw_json,
                deferred_response,
            )
            .await
    }

    pub fn detach_service_worker_runtime_inspector_session(
        &self,
        version_id: u64,
        inspector_session_id: Option<String>,
    ) -> bool {
        self.inner
            .service_worker_runtime
            .detach_runtime_inspector_session(
                ServiceWorkerVersionId::from_u64_for_binding(version_id),
                inspector_session_id,
            )
    }

    pub fn unregister_service_worker_scope_for_devtools(
        &self,
        scope_url: &url::Url,
    ) -> Result<bool, String> {
        self.inner
            .service_worker_runtime
            .devtools_unregister_scope(scope_url)
    }

    pub fn start_service_worker_for_devtools(&self, scope_url: &url::Url) -> Result<bool, String> {
        self.inner
            .service_worker_runtime
            .devtools_start_worker_for_scope(scope_url)
    }

    pub fn stop_service_worker_for_devtools(&self, version_id: u64) -> Result<bool, String> {
        self.inner
            .service_worker_runtime
            .devtools_stop_worker_version(ServiceWorkerVersionId::from_u64_for_binding(version_id))
    }

    pub fn stop_all_service_workers_for_devtools(&self) -> Result<usize, String> {
        self.inner
            .service_worker_runtime
            .devtools_stop_all_workers()
    }

    pub fn skip_waiting_service_worker_for_devtools(
        &self,
        scope_url: &url::Url,
    ) -> Result<bool, String> {
        self.inner
            .service_worker_runtime
            .devtools_skip_waiting_for_scope(scope_url)
    }

    pub fn update_service_worker_registration_for_devtools(
        &self,
        scope_url: &url::Url,
    ) -> Result<bool, String> {
        self.inner
            .service_worker_runtime
            .devtools_update_registration_for_scope(scope_url, self.clone())
    }

    pub fn set_service_worker_force_update_on_page_load_for_devtools(&self, force_update: bool) {
        self.inner
            .service_worker_runtime
            .set_force_update_on_page_load_for_devtools(force_update);
    }

    pub fn service_worker_force_update_on_page_load_for_devtools(&self) -> bool {
        self.inner
            .service_worker_runtime
            .force_update_on_page_load_for_devtools()
    }

    pub fn controlled_service_worker_window_client_urls_for_devtools(
        &self,
        registration_id: u64,
        version_id: u64,
    ) -> Vec<String> {
        self.inner
            .service_worker_runtime
            .controlled_window_client_urls_for_version_for_devtools(
                ServiceWorkerRegistrationId::from_u64_for_binding(registration_id),
                ServiceWorkerVersionId::from_u64_for_binding(version_id),
            )
    }

    pub fn controlled_service_worker_window_client_ids_for_devtools(
        &self,
        registration_id: u64,
        version_id: u64,
    ) -> Vec<u64> {
        self.inner
            .service_worker_runtime
            .controlled_window_client_ids_for_version_for_devtools(
                ServiceWorkerRegistrationId::from_u64_for_binding(registration_id),
                ServiceWorkerVersionId::from_u64_for_binding(version_id),
            )
    }

    pub fn set_service_worker_pause_on_start_for_devtools(&self, pause: bool) {
        self.inner
            .service_worker_runtime
            .set_pause_new_workers_on_start_for_devtools(pause);
    }

    pub fn set_service_worker_related_pause_on_start_policies_for_devtools(
        &self,
        policies: Vec<(u64, u64, String, String)>,
    ) {
        self.inner
            .service_worker_runtime
            .set_related_pause_on_start_policies_for_devtools(policies);
    }

    pub fn set_service_worker_pause_on_start_for_version_for_devtools(
        &self,
        version_id: u64,
        pause: bool,
    ) -> bool {
        self.inner
            .service_worker_runtime
            .set_pause_on_start_for_version_for_devtools(
                ServiceWorkerVersionId::from_u64_for_binding(version_id),
                pause,
            )
    }

    pub fn service_worker_pause_on_start_for_devtools(&self) -> bool {
        self.inner
            .service_worker_runtime
            .pause_new_workers_on_start_for_devtools()
    }

    pub fn set_service_worker_devtools_attached(&self, version_id: u64, attached: bool) {
        self.inner
            .service_worker_runtime
            .set_devtools_attached_for_version(
                ServiceWorkerVersionId::from_u64_for_binding(version_id),
                attached,
            );
    }

    pub fn run_service_worker_if_waiting_for_debugger_for_devtools(&self, version_id: u64) -> bool {
        self.inner
            .service_worker_runtime
            .devtools_run_if_waiting_for_debugger(ServiceWorkerVersionId::from_u64_for_binding(
                version_id,
            ))
    }

    pub fn release_all_service_workers_waiting_for_debugger_for_devtools(&self) -> usize {
        self.inner
            .service_worker_runtime
            .devtools_release_all_workers_waiting_for_debugger()
    }

    pub fn deliver_push_message_for_devtools(
        &self,
        origin: &url::Url,
        registration_id: u64,
        data: Option<Vec<u8>>,
    ) -> Result<bool, String> {
        self.inner
            .service_worker_runtime
            .devtools_deliver_push_message(
                origin,
                ServiceWorkerRegistrationId::from_u64_for_binding(registration_id),
                data,
            )
    }

    pub fn dispatch_sync_event_for_devtools(
        &self,
        origin: &url::Url,
        registration_id: u64,
        tag: String,
        last_chance: bool,
    ) -> Result<bool, String> {
        self.inner
            .service_worker_runtime
            .devtools_dispatch_sync_event(
                origin,
                ServiceWorkerRegistrationId::from_u64_for_binding(registration_id),
                tag,
                last_chance,
            )
    }

    pub fn dispatch_periodic_sync_event_for_devtools(
        &self,
        origin: &url::Url,
        registration_id: u64,
        tag: String,
    ) -> Result<bool, String> {
        self.inner
            .service_worker_runtime
            .devtools_dispatch_periodic_sync_event(
                origin,
                ServiceWorkerRegistrationId::from_u64_for_binding(registration_id),
                tag,
            )
    }
}
