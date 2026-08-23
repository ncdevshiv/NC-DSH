use serde_json::{Value, json};

use super::service::SharedWorkerRuntimeService;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RendererSharedWorkerRuntimeDiagnostics {
    pub matching_entry_count: usize,
    pub loading_instance_count: usize,
    pub running_instance_count: usize,
    pub client_count: usize,
    pub loading_host_count: usize,
    pub running_worker_isolate_count: usize,
    pub pending_service_lane_event_count: usize,
}

impl SharedWorkerRuntimeService {
    pub(crate) fn diagnostics_snapshot(&self) -> RendererSharedWorkerRuntimeDiagnostics {
        let matching = self.matching_diagnostics();
        RendererSharedWorkerRuntimeDiagnostics {
            matching_entry_count: matching.entry_count,
            loading_instance_count: matching.loading_instance_count,
            running_instance_count: matching.running_instance_count,
            client_count: matching.client_count,
            loading_host_count: self.loading_host_count(),
            running_worker_isolate_count: matching.running_instance_count,
            pending_service_lane_event_count: self.pending_service_lane_event_count(),
        }
    }

    pub(crate) fn moli_memory_diagnostics(&self) -> Value {
        let diagnostics = self.diagnostics_snapshot();
        json!({
            "matchingEntryCount": diagnostics.matching_entry_count,
            "loadingInstanceCount": diagnostics.loading_instance_count,
            "runningInstanceCount": diagnostics.running_instance_count,
            "clientCount": diagnostics.client_count,
            "loadingHostCount": diagnostics.loading_host_count,
            "runningWorkerIsolateCount": diagnostics.running_worker_isolate_count,
            "pendingServiceLaneEventCount": diagnostics.pending_service_lane_event_count,
        })
    }
}
