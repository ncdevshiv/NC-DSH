/// Opaque identifier for one SharedWorker client connection.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct SharedWorkerClientId(u64);

impl SharedWorkerClientId {
    /// Build a client id from the registry allocator.
    pub(crate) fn new(id: u64) -> Self {
        Self(id)
    }

    /// Return the numeric id for renderer-side private slots and diagnostics.
    pub fn as_u64(self) -> u64 {
        self.0
    }
}

/// Opaque identifier for the browsing context that owns client connections.
///
/// A single context can create multiple `SharedWorker` wrappers connected to
/// the same worker. Chromium keeps those port-level connections separate while
/// aggregating observer/client-active state by render frame; this id is the
/// renderer-neutral equivalent of that frame/context key.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct SharedWorkerClientOwnerId(u64);

impl SharedWorkerClientOwnerId {
    /// Rebuild an owner id from embedder/browser-context state.
    pub fn from_u64(id: u64) -> Self {
        assert!(id != 0, "SharedWorker client owner ids must be non-zero");
        Self(id)
    }

    /// Return the numeric id for diagnostics.
    pub fn as_u64(self) -> u64 {
        self.0
    }

    pub(crate) fn unique_for_client(client_id: SharedWorkerClientId) -> Self {
        Self(client_id.as_u64())
    }
}

/// Opaque identifier for one SharedWorker instance slot.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct SharedWorkerInstanceId(u64);

impl SharedWorkerInstanceId {
    /// Build an instance id from the registry allocator.
    pub(crate) fn new(id: u64) -> Self {
        Self(id)
    }

    /// Rebuild an instance id from renderer/CDP state that only carries the
    /// diagnostic numeric form.
    pub fn from_u64(id: u64) -> Self {
        Self(id)
    }

    /// Return the numeric id for diagnostics.
    pub fn as_u64(self) -> u64 {
        self.0
    }
}
