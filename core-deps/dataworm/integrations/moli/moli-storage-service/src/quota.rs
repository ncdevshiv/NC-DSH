use std::{collections::BTreeSet, sync::Arc};

use parking_lot::{Condvar, Mutex};

use crate::StorageBucketLocator;

/// Partition-owned serializer for aggregate bucket quota commits.
///
/// Backend staging may happen independently, but every commit which can grow
/// a bucket must hold one reservation while it takes a fresh aggregate
/// usage snapshot and publishes the backend mutation. Reservations are scoped
/// by persistent bucket locator, so unrelated buckets remain independent.
#[derive(Clone, Debug, Default)]
pub(crate) struct StorageQuotaCoordinator {
    inner: Arc<StorageQuotaCoordinatorInner>,
}

#[derive(Debug, Default)]
struct StorageQuotaCoordinatorInner {
    active: Mutex<BTreeSet<StorageBucketLocator>>,
    released: Condvar,
}

impl StorageQuotaCoordinator {
    pub(crate) fn try_reserve(
        &self,
        locator: &StorageBucketLocator,
    ) -> Option<StorageQuotaReservation> {
        let mut active = self.inner.active.lock();
        if active.contains(locator) {
            return None;
        }
        let inserted = active.insert(locator.clone());
        debug_assert!(inserted, "free bucket quota reservation must be insertable");
        Some(StorageQuotaReservation {
            coordinator: self.clone(),
            locator: Some(locator.clone()),
        })
    }

    pub(crate) fn reserve(&self, locator: &StorageBucketLocator) -> StorageQuotaReservation {
        let mut active = self.inner.active.lock();
        while active.contains(locator) {
            self.inner.released.wait(&mut active);
        }
        let inserted = active.insert(locator.clone());
        debug_assert!(inserted, "waited bucket quota reservation must be free");
        StorageQuotaReservation {
            coordinator: self.clone(),
            locator: Some(locator.clone()),
        }
    }

    fn release(&self, locator: StorageBucketLocator) {
        let mut active = self.inner.active.lock();
        let removed = active.remove(&locator);
        debug_assert!(removed, "released bucket quota reservation must be active");
        drop(active);
        self.inner.released.notify_all();
    }
}

/// Owned reservation for one bucket's aggregate quota commit window.
///
/// The reservation is `Send`, so an asynchronous storage task can acquire it
/// on the storage worker and release it before its owner-thread completion is
/// delivered. Dropping it on any error or panic always releases the bucket.
#[derive(Debug)]
pub struct StorageQuotaReservation {
    coordinator: StorageQuotaCoordinator,
    locator: Option<StorageBucketLocator>,
}

impl StorageQuotaReservation {
    /// Return the exact persistent bucket protected by this reservation.
    pub fn locator(&self) -> &StorageBucketLocator {
        self.locator
            .as_ref()
            .expect("live quota reservation must retain its locator")
    }
}

impl Drop for StorageQuotaReservation {
    fn drop(&mut self) {
        if let Some(locator) = self.locator.take() {
            self.coordinator.release(locator);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{
            Arc, Barrier,
            atomic::{AtomicUsize, Ordering},
            mpsc,
        },
        thread,
        time::Duration,
    };

    use crate::{StorageBucketId, StorageBucketLocator};

    use super::StorageQuotaCoordinator;

    fn named_bucket(id: u64) -> StorageBucketLocator {
        StorageBucketLocator::named(
            "storage-key:v1;origin=https://quota.example",
            StorageBucketId::new(id).expect("test bucket ID should be non-zero"),
        )
    }

    #[test]
    fn same_bucket_quota_reservations_are_exclusive_and_drop_releases() {
        let coordinator = StorageQuotaCoordinator::default();
        let locator = named_bucket(1);
        let first = coordinator.reserve(&locator);
        let (entered_tx, entered_rx) = mpsc::channel();
        let waiting_coordinator = coordinator.clone();
        let waiting_locator = locator.clone();
        let waiter = thread::spawn(move || {
            let _second = waiting_coordinator.reserve(&waiting_locator);
            entered_tx
                .send(())
                .expect("test receiver should remain live");
        });

        assert!(entered_rx.recv_timeout(Duration::from_millis(50)).is_err());
        drop(first);
        entered_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("dropping the first reservation should release the waiter");
        waiter.join().expect("quota waiter should not panic");
    }

    #[test]
    fn different_bucket_quota_reservations_can_overlap() {
        let coordinator = StorageQuotaCoordinator::default();
        let active = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let overlap = Arc::new(Barrier::new(3));
        let (entered_tx, entered_rx) = mpsc::channel();
        let mut workers = Vec::new();
        for id in [1, 2] {
            let coordinator = coordinator.clone();
            let active = active.clone();
            let peak = peak.clone();
            let overlap = overlap.clone();
            let entered_tx = entered_tx.clone();
            workers.push(thread::spawn(move || {
                let _reservation = coordinator.reserve(&named_bucket(id));
                let now = active.fetch_add(1, Ordering::SeqCst) + 1;
                peak.fetch_max(now, Ordering::SeqCst);
                entered_tx
                    .send(())
                    .expect("test receiver should remain live");
                overlap.wait();
                active.fetch_sub(1, Ordering::SeqCst);
            }));
        }
        drop(entered_tx);

        entered_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("first bucket should enter");
        entered_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("second bucket should enter independently");
        overlap.wait();
        for worker in workers {
            worker.join().expect("quota worker should not panic");
        }
        assert_eq!(peak.load(Ordering::SeqCst), 2);
    }
}
