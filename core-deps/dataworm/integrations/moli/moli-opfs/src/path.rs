use std::fmt;

use serde::{Deserialize, Serialize};

use crate::{OpfsError, OpfsResult};

/// Opaque identity supplied by the storage service for one filesystem root.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct OpfsBucketKey(String);

impl OpfsBucketKey {
    /// Build a key from a non-empty service-owned serialization.
    pub fn new(value: impl Into<String>) -> OpfsResult<Self> {
        let value = value.into();
        if value.is_empty() {
            return Err(OpfsError::CorruptCatalog(
                "OPFS bucket key must not be empty".to_owned(),
            ));
        }
        Ok(Self(value))
    }

    /// Return the opaque serialization used only for equality and hashing.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Kind of an OPFS namespace entry.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EntryKind {
    Directory,
    File,
}

impl fmt::Display for EntryKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Directory => f.write_str("directory"),
            Self::File => f.write_str("file"),
        }
    }
}

/// Validated virtual path relative to an OPFS root.
#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct OpfsPath(Vec<String>);

impl OpfsPath {
    /// Return the root path.
    pub const fn root() -> Self {
        Self(Vec::new())
    }

    /// Build a path after validating every component.
    pub fn from_components(components: impl IntoIterator<Item = String>) -> OpfsResult<Self> {
        let components = components.into_iter().collect::<Vec<_>>();
        for component in &components {
            validate_name(component)?;
        }
        Ok(Self(components))
    }

    /// Return the path components.
    pub fn components(&self) -> &[String] {
        &self.0
    }

    /// Return whether this is the root path.
    pub fn is_root(&self) -> bool {
        self.0.is_empty()
    }

    /// Return the final component, or the empty root name.
    pub fn name(&self) -> &str {
        self.0.last().map(String::as_str).unwrap_or("")
    }

    /// Return a child path after validating the child name.
    pub fn child(&self, name: &str) -> OpfsResult<Self> {
        validate_name(name)?;
        let mut components = self.0.clone();
        components.push(name.to_owned());
        Ok(Self(components))
    }

    /// Return the parent path, or `None` for root.
    pub fn parent(&self) -> Option<Self> {
        (!self.0.is_empty()).then(|| Self(self.0[..self.0.len() - 1].to_vec()))
    }

    /// Return whether this path is equal to or an ancestor of `other`.
    pub fn is_ancestor_of_or_equal(&self, other: &Self) -> bool {
        other.0.starts_with(&self.0)
    }

    /// Return a slash-prefixed diagnostic serialization.
    pub fn display(&self) -> String {
        if self.is_root() {
            "/".to_owned()
        } else {
            format!("/{}", self.0.join("/"))
        }
    }
}

/// Validate one File System Access path component.
pub fn validate_name(name: &str) -> OpfsResult<()> {
    if name.is_empty() || matches!(name, "." | "..") || name.contains(['/', '\\']) {
        return Err(OpfsError::InvalidName(name.to_owned()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{OpfsPath, validate_name};

    #[test]
    fn path_validation_rejects_only_reserved_components() {
        for invalid in ["", ".", "..", "a/b", "a\\b"] {
            assert!(validate_name(invalid).is_err(), "{invalid:?}");
        }
        for valid in ["name", "...", "a:b", "nul\0name", "目录", " leading "] {
            assert!(validate_name(valid).is_ok(), "{valid:?}");
        }
    }

    #[test]
    fn virtual_paths_keep_root_and_component_semantics() {
        let root = OpfsPath::root();
        let directory = root.child("dir").unwrap();
        let file = directory.child("file.txt").unwrap();
        assert_eq!(root.name(), "");
        assert_eq!(file.display(), "/dir/file.txt");
        assert_eq!(file.parent(), Some(directory.clone()));
        assert!(directory.is_ancestor_of_or_equal(&file));
        assert!(!file.is_ancestor_of_or_equal(&directory));
    }
}
