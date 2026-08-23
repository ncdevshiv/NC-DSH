use std::collections::HashMap;

use moli_shared_worker::{SharedWorkerInstanceId, SharedWorkerInstanceRemoval};
use parking_lot::Mutex;

use super::{host::SharedRendererSharedWorkerHost, host_removal::SharedWorkerRemovedHost};

#[derive(Default)]
pub(super) struct SharedWorkerHostStore {
    loading_hosts: Mutex<HashMap<SharedWorkerInstanceId, SharedRendererSharedWorkerHost>>,
}

impl SharedWorkerHostStore {
    pub(super) fn loading_host(
        &self,
        instance_id: SharedWorkerInstanceId,
    ) -> Option<SharedRendererSharedWorkerHost> {
        self.loading_hosts.lock().get(&instance_id).cloned()
    }

    pub(super) fn insert_loading_host(
        &self,
        instance_id: SharedWorkerInstanceId,
        host: SharedRendererSharedWorkerHost,
    ) {
        self.loading_hosts.lock().insert(instance_id, host);
    }

    pub(super) fn remove_loading_host(
        &self,
        instance_id: SharedWorkerInstanceId,
    ) -> Option<SharedRendererSharedWorkerHost> {
        self.loading_hosts.lock().remove(&instance_id)
    }

    pub(super) fn loading_host_count(&self) -> usize {
        self.loading_hosts.lock().len()
    }

    pub(super) fn take_context_shutdown_hosts<F>(
        &self,
        remove_all_instances: F,
    ) -> Vec<SharedWorkerRemovedHost>
    where
        F: FnOnce() -> Vec<SharedWorkerInstanceRemoval<SharedRendererSharedWorkerHost>>,
    {
        // Context shutdown must classify registry removals and drain loading
        // hosts while this lock is held, otherwise a concurrent load
        // completion or client cancellation can steal a loading host before
        // pending clients are sent their terminal close event.
        let mut loading_hosts = self.loading_hosts.lock();
        let removals = remove_all_instances();
        removed_hosts_for_context_shutdown(removals, &mut loading_hosts)
    }

    #[cfg(test)]
    pub(super) fn is_empty(&self) -> bool {
        self.loading_hosts.lock().is_empty()
    }
}

fn removed_hosts_for_context_shutdown(
    removals: Vec<SharedWorkerInstanceRemoval<SharedRendererSharedWorkerHost>>,
    loading_hosts: &mut HashMap<SharedWorkerInstanceId, SharedRendererSharedWorkerHost>,
) -> Vec<SharedWorkerRemovedHost> {
    let mut removed_hosts = Vec::new();
    for removal in removals {
        let SharedWorkerInstanceRemoval::Removed {
            clients,
            instance_id,
            instance,
            ..
        } = removal
        else {
            unreachable!("bulk instance removal never yields Missing")
        };

        if let Some(host) = instance {
            removed_hosts.push(SharedWorkerRemovedHost::Running { host, clients });
        } else if let Some(host) = loading_hosts.remove(&instance_id) {
            removed_hosts.push(SharedWorkerRemovedHost::Loading { host, clients });
        }
    }
    removed_hosts.extend(
        loading_hosts
            .drain()
            .map(|(_, host)| SharedWorkerRemovedHost::Loading {
                host,
                clients: Vec::new(),
            }),
    );
    removed_hosts
}
