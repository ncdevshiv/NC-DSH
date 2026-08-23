use std::collections::HashSet;

use moli_shared_worker::{
    SharedWorkerClientOwnerEvent, SharedWorkerClientOwnerId, SharedWorkerInstanceId,
};
use parking_lot::Mutex;

/// Runtime-side projection of owner-level SharedWorker client lifecycle.
///
/// Chromium emits client observer notifications from the browser-side service
/// only when a frame's refcount for a worker crosses 0 <-> 1. The neutral
/// registry already computes those transitions atomically; this store is the
/// renderer runtime hook that future observer, permission, or active-client
/// code should consume instead of recounting JS wrappers.
#[derive(Default)]
pub(super) struct SharedWorkerClientOwnerLifecycleStore {
    active_owners: Mutex<HashSet<SharedWorkerClientOwnerLifecycleKey>>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct SharedWorkerClientOwnerLifecycleKey {
    instance_id: SharedWorkerInstanceId,
    owner_id: SharedWorkerClientOwnerId,
}

impl SharedWorkerClientOwnerLifecycleStore {
    pub(super) fn apply_events(
        &self,
        events: impl IntoIterator<Item = SharedWorkerClientOwnerEvent>,
    ) {
        let mut active_owners = self.active_owners.lock();
        for event in events {
            match event {
                SharedWorkerClientOwnerEvent::FirstClientAdded {
                    instance_id,
                    owner_id,
                } => {
                    active_owners.insert(SharedWorkerClientOwnerLifecycleKey {
                        instance_id,
                        owner_id,
                    });
                }
                SharedWorkerClientOwnerEvent::LastClientRemoved {
                    instance_id,
                    owner_id,
                } => {
                    active_owners.remove(&SharedWorkerClientOwnerLifecycleKey {
                        instance_id,
                        owner_id,
                    });
                }
            }
        }
    }

    #[cfg(test)]
    pub(super) fn active_owner_ids_for_instance(
        &self,
        instance_id: SharedWorkerInstanceId,
    ) -> Vec<SharedWorkerClientOwnerId> {
        let mut owners = self
            .active_owners
            .lock()
            .iter()
            .filter_map(|key| (key.instance_id == instance_id).then_some(key.owner_id))
            .collect::<Vec<_>>();
        owners.sort_by_key(|owner_id| owner_id.as_u64());
        owners
    }

    #[cfg(test)]
    pub(super) fn is_empty(&self) -> bool {
        self.active_owners.lock().is_empty()
    }
}

#[cfg(test)]
mod tests {
    use moli_shared_worker::{
        SharedWorkerClientOwnerEvent, SharedWorkerClientOwnerId, SharedWorkerInstanceId,
    };

    use super::SharedWorkerClientOwnerLifecycleStore;

    #[test]
    fn owner_events_track_active_owner_projection() {
        let store = SharedWorkerClientOwnerLifecycleStore::default();
        let instance_id = SharedWorkerInstanceId::from_u64(7);
        let owner_id = SharedWorkerClientOwnerId::from_u64(11);

        store.apply_events([SharedWorkerClientOwnerEvent::FirstClientAdded {
            instance_id,
            owner_id,
        }]);
        assert_eq!(
            store.active_owner_ids_for_instance(instance_id),
            vec![owner_id]
        );

        store.apply_events([SharedWorkerClientOwnerEvent::LastClientRemoved {
            instance_id,
            owner_id,
        }]);
        assert!(store.is_empty());
    }
}
