use std::sync::Arc;

use moli_shared_worker::SharedWorkerInstanceId;

use super::{
    host::{RendererSharedWorkerHost, SharedRendererSharedWorkerHost},
    service::{SharedWorkerRuntimeService, WeakSharedWorkerRuntimeService},
};

impl SharedWorkerRuntimeService {
    pub(super) fn running_host_for_instance(
        &self,
        instance_id: SharedWorkerInstanceId,
    ) -> Option<SharedRendererSharedWorkerHost> {
        self.running_matching_host(instance_id)
    }

    pub(super) fn route_running_host(
        &self,
        instance_id: SharedWorkerInstanceId,
        route: impl FnOnce(&RendererSharedWorkerHost) -> bool,
    ) -> bool {
        let Some(host) = self.running_host_for_route(instance_id) else {
            return false;
        };
        route(&host)
    }

    fn running_host_for_route(
        &self,
        instance_id: SharedWorkerInstanceId,
    ) -> Option<SharedRendererSharedWorkerHost> {
        self.running_host_for_instance(instance_id)
    }
}

impl WeakSharedWorkerRuntimeService {
    pub(super) fn running_host_for_instance(
        &self,
        instance_id: SharedWorkerInstanceId,
    ) -> Option<SharedRendererSharedWorkerHost> {
        self.upgrade()
            .and_then(|service| service.running_host_for_instance(instance_id))
    }

    pub(super) fn is_running_host(&self, host: &SharedRendererSharedWorkerHost) -> bool {
        self.running_host_for_instance(host.instance_id())
            .is_some_and(|running| Arc::ptr_eq(&running, host))
    }
}
