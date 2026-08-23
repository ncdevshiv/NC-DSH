use std::{io, path::PathBuf};

use thiserror::Error;

use crate::EntryKind;

/// Result type returned by the OPFS backend.
pub type OpfsResult<T> = Result<T, OpfsError>;

/// Renderer-neutral failures mapped to DOMException/TypeError by API adapters.
#[derive(Debug, Error)]
pub enum OpfsError {
    /// One path component is not a valid File System Access API name.
    #[error("invalid OPFS name `{0}`")]
    InvalidName(String),
    /// The requested virtual entry does not exist.
    #[error("OPFS entry `{0}` was not found")]
    NotFound(String),
    /// The entry exists with a different kind.
    #[error("OPFS entry `{path}` is {actual}, expected {expected}")]
    TypeMismatch {
        path: String,
        expected: EntryKind,
        actual: EntryKind,
    },
    /// A non-empty directory cannot be removed without recursive mode.
    #[error("OPFS directory `{0}` is not empty")]
    DirectoryNotEmpty(String),
    /// The requested mutation would make the namespace invalid.
    #[error("invalid OPFS modification: {0}")]
    InvalidModification(String),
    /// An active writer/sync handle conflicts with this operation.
    #[error("OPFS entry is locked: {0}")]
    NoModificationAllowed(String),
    /// A writer or sync handle has already been closed/aborted.
    #[error("OPFS session is no longer active")]
    InvalidState,
    /// The requested logical usage exceeds the caller-provided bucket limit.
    #[error("OPFS quota exceeded: requested {requested} bytes, quota {quota} bytes")]
    QuotaExceeded { quota: u64, requested: u64 },
    /// Persistent catalog data failed integrity validation.
    #[error("corrupt OPFS catalog: {0}")]
    CorruptCatalog(String),
    /// JSON serialization or parsing failed.
    #[error("failed to process OPFS catalog JSON: {0}")]
    CatalogJson(#[from] serde_json::Error),
    /// A host filesystem operation failed inside the sandbox backend.
    #[error("failed to {operation} OPFS backend path `{path}`: {source}")]
    Io {
        operation: &'static str,
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

impl OpfsError {
    pub(crate) fn io(operation: &'static str, path: impl Into<PathBuf>, source: io::Error) -> Self {
        Self::Io {
            operation,
            path: path.into(),
            source,
        }
    }
}
