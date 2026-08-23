use crate::{OpfsMutationLease, OpfsSyncAccessHandleId, OpfsWritableId, SharedStorageService};

/// Storage-sequence owner for a committed move/remove lock lease.
///
/// Dropping this completion guard appends the backend lock release to the
/// ordered storage sequence. Operations submitted before Promise settlement
/// therefore observe the mutation lock, while operations submitted by the
/// settled Promise's continuation run after its release.
#[derive(Debug)]
pub struct StorageOpfsMutationLease {
    service: SharedStorageService,
    lease: Option<OpfsMutationLease>,
}

impl StorageOpfsMutationLease {
    /// Transfer one backend mutation lease to the storage sequence owner.
    pub fn new(service: SharedStorageService, lease: OpfsMutationLease) -> Self {
        Self {
            service,
            lease: Some(lease),
        }
    }
}

impl Drop for StorageOpfsMutationLease {
    fn drop(&mut self) {
        let Some(lease) = self.lease.take() else {
            return;
        };
        let _ = self.service.dispatch_opfs(move |_| drop(lease), |_| {});
    }
}

/// Storage-owner cleanup guard for an active asynchronous OPFS writer.
///
/// Renderer completion payloads use this guard while a writer has not yet
/// been attached to a live Web-visible wrapper. Dropping the guard never runs
/// backend IO on the caller: it appends an abort to the partition's ordered
/// storage sequence instead.
#[derive(Debug)]
pub struct StorageOpfsWritableLease {
    service: SharedStorageService,
    writer_id: Option<OpfsWritableId>,
}

impl StorageOpfsWritableLease {
    /// Own cleanup responsibility for `writer_id` on `service`.
    pub fn new(service: SharedStorageService, writer_id: OpfsWritableId) -> Self {
        Self {
            service,
            writer_id: Some(writer_id),
        }
    }

    /// Return the writer represented by this lease.
    pub fn writer_id(&self) -> OpfsWritableId {
        self.writer_id
            .expect("an active OPFS writable lease must have a writer id")
    }

    /// Release cleanup responsibility after a live owner consumed the result.
    pub fn disarm(&mut self) {
        self.writer_id = None;
    }
}

impl Drop for StorageOpfsWritableLease {
    fn drop(&mut self) {
        let Some(writer_id) = self.writer_id.take() else {
            return;
        };
        let _ = self
            .service
            .dispatch_opfs(move |opfs| opfs.abort_writable(writer_id), |_| {});
    }
}

/// Storage-owner cleanup guard for an active synchronous OPFS access session.
///
/// The guard is used while an asynchronously-created session is in transit to
/// its Worker realm and by the final Web wrapper. Dropping it appends a close
/// to the ordered storage sequence so dirty sync data is committed and the
/// path lock is released without blocking renderer teardown.
#[derive(Debug)]
pub struct StorageOpfsSyncAccessLease {
    service: SharedStorageService,
    handle_id: Option<OpfsSyncAccessHandleId>,
}

impl StorageOpfsSyncAccessLease {
    /// Own cleanup responsibility for `handle_id` on `service`.
    pub fn new(service: SharedStorageService, handle_id: OpfsSyncAccessHandleId) -> Self {
        Self {
            service,
            handle_id: Some(handle_id),
        }
    }

    /// Return the sync access session represented by this lease.
    pub fn handle_id(&self) -> OpfsSyncAccessHandleId {
        self.handle_id
            .expect("an active OPFS sync access lease must have a handle id")
    }
}

impl Drop for StorageOpfsSyncAccessLease {
    fn drop(&mut self) {
        let Some(handle_id) = self.handle_id.take() else {
            return;
        };
        let _ = self
            .service
            .dispatch_opfs(move |opfs| opfs.close_sync(handle_id, None), |_| {});
    }
}
