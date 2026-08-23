use crate::{OpfsBucketKey, OpfsPath, staging::WritableStaging};

/// Opaque ID of an active asynchronous writable session.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct OpfsWritableId(pub(crate) u64);

impl OpfsWritableId {
    /// Rebuild an ID stored in a renderer wrapper.
    pub const fn from_raw(value: u64) -> Option<Self> {
        if value == 0 { None } else { Some(Self(value)) }
    }

    /// Return the integer representation used by native wrapper slots.
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Opaque ID of an active synchronous access session.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct OpfsSyncAccessHandleId(pub(crate) u64);

impl OpfsSyncAccessHandleId {
    /// Rebuild an ID stored in a renderer wrapper.
    pub const fn from_raw(value: u64) -> Option<Self> {
        if value == 0 { None } else { Some(Self(value)) }
    }

    /// Return the integer representation used by native wrapper slots.
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Locking mode for `createWritable()`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum WritableMode {
    /// Multiple writers receive independent staging data and commit on close.
    #[default]
    Siloed,
    /// Reject creation while any overlapping writer/access handle exists.
    Exclusive,
}

/// Mutation accepted by an active writable session.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WritableCommand {
    Write {
        data: Vec<u8>,
        position: Option<u64>,
    },
    Seek(u64),
    Truncate(u64),
}

/// Locking mode for a synchronous access handle.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SyncAccessMode {
    #[default]
    Readwrite,
    ReadOnly,
    ReadwriteUnsafe,
}

#[derive(Debug)]
pub(crate) struct WritableSession {
    pub bucket: OpfsBucketKey,
    pub path: OpfsPath,
    pub cursor: u64,
    pub staging: WritableStaging,
    pub committed_size: u64,
}

#[derive(Debug)]
pub(crate) struct SyncAccessSession {
    pub bucket: OpfsBucketKey,
    pub path: OpfsPath,
    pub entry_id: u64,
    pub backing_id: u64,
    pub cursor: u64,
    pub mode: SyncAccessMode,
}
