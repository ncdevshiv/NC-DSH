//! Renderer-neutral Origin Private File System backend.
//!
//! This crate owns virtual paths, namespace/catalog integrity, file contents,
//! atomic writable replacement, shared live sync backings, sessions, recovery,
//! and hierarchical locks. It has no V8, DOM, page, worker, protocol, Storage
//! Buckets, or profile-manifest dependencies. Callers supply an opaque bucket
//! key and never receive a host filesystem path.

mod catalog;
mod error;
#[cfg(test)]
mod fault_injection;
mod locks;
mod mutation;
mod path;
mod sessions;
mod staging;
mod store;
mod sync_backing;

pub use error::{OpfsError, OpfsResult};
pub use mutation::OpfsMutationLease;
pub use path::{EntryKind, OpfsBucketKey, OpfsPath, validate_name};
pub use sessions::{
    OpfsSyncAccessHandleId, OpfsWritableId, SyncAccessMode, WritableCommand, WritableMode,
};
pub use store::{DirectoryEntry, FileSnapshot, FileSnapshotIdentity, Opfs};
