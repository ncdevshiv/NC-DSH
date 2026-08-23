use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use typed_num::Num;

use crate::{EntryKind, OpfsError, OpfsPath, OpfsResult, validate_name};

pub(crate) const ROOT_ENTRY_ID: u64 = 0;
const PATH_CREATION_QUOTA_COST: u64 = 146;
const PATH_BYTE_QUOTA_COST: u64 = 2;
type CatalogVersion = Num<1>;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Catalog {
    version: CatalogVersion,
    next_entry_id: u64,
    next_backing_id: u64,
    next_version_id: u64,
    entries: BTreeMap<u64, CatalogEntry>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CatalogEntry {
    pub parent_id: u64,
    pub name: String,
    pub kind: EntryKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backing_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version_id: Option<u64>,
    pub size: u64,
    pub modified_ms: u64,
}

impl Catalog {
    pub(crate) fn new(now_ms: u64) -> Self {
        Self {
            version: CatalogVersion::default(),
            next_entry_id: 1,
            next_backing_id: 1,
            next_version_id: 1,
            entries: BTreeMap::from([(
                ROOT_ENTRY_ID,
                CatalogEntry {
                    parent_id: ROOT_ENTRY_ID,
                    name: String::new(),
                    kind: EntryKind::Directory,
                    backing_id: None,
                    version_id: None,
                    size: 0,
                    modified_ms: now_ms,
                },
            )]),
        }
    }

    pub(crate) fn from_bytes(bytes: &[u8]) -> OpfsResult<Self> {
        let catalog: Self = serde_json::from_slice(bytes)?;
        catalog.validate()?;
        Ok(catalog)
    }

    pub(crate) fn to_bytes(&self) -> OpfsResult<Vec<u8>> {
        self.validate()?;
        Ok(serde_json::to_vec_pretty(self)?)
    }

    /// Return an empty namespace without reusing any previously observable
    /// entry, backing, or observable version identity.
    ///
    /// `FileSystemFileHandle.getFile()` snapshots retain these identities to
    /// detect replacement and modification. Resetting either allocator during
    /// a bucket clear could make an old snapshot valid again after a same-path
    /// file is recreated.
    pub(crate) fn cleared(&self, now_ms: u64) -> Self {
        let mut cleared = Self::new(now_ms);
        cleared.next_entry_id = self.next_entry_id;
        cleared.next_backing_id = self.next_backing_id;
        cleared.next_version_id = self.next_version_id;
        cleared
    }

    pub(crate) fn entry(&self, id: u64) -> OpfsResult<&CatalogEntry> {
        self.entries
            .get(&id)
            .ok_or_else(|| OpfsError::CorruptCatalog(format!("missing entry id {id}")))
    }

    pub(crate) fn entry_mut(&mut self, id: u64) -> OpfsResult<&mut CatalogEntry> {
        self.entries
            .get_mut(&id)
            .ok_or_else(|| OpfsError::CorruptCatalog(format!("missing entry id {id}")))
    }

    pub(crate) fn resolve(&self, path: &OpfsPath) -> OpfsResult<u64> {
        let mut current = ROOT_ENTRY_ID;
        for component in path.components() {
            current = self
                .child_id(current, component)
                .ok_or_else(|| OpfsError::NotFound(path.display()))?;
        }
        Ok(current)
    }

    pub(crate) fn resolve_kind(&self, path: &OpfsPath, expected: EntryKind) -> OpfsResult<u64> {
        let id = self.resolve(path)?;
        let actual = self.entry(id)?.kind;
        if actual != expected {
            return Err(OpfsError::TypeMismatch {
                path: path.display(),
                expected,
                actual,
            });
        }
        Ok(id)
    }

    pub(crate) fn child_id(&self, parent_id: u64, name: &str) -> Option<u64> {
        self.entries.iter().find_map(|(id, entry)| {
            (entry.parent_id == parent_id && entry.name == name).then_some(*id)
        })
    }

    pub(crate) fn children(&self, parent_id: u64) -> Vec<(u64, &CatalogEntry)> {
        let mut children = self
            .entries
            .iter()
            .filter(|(id, entry)| **id != ROOT_ENTRY_ID && entry.parent_id == parent_id)
            .map(|(id, entry)| (*id, entry))
            .collect::<Vec<_>>();
        children.sort_by(|left, right| left.1.name.cmp(&right.1.name));
        children
    }

    pub(crate) fn create_entry(
        &mut self,
        parent_id: u64,
        name: String,
        kind: EntryKind,
        now_ms: u64,
    ) -> OpfsResult<(u64, Option<u64>)> {
        validate_name(&name)?;
        let parent = self.entry(parent_id)?;
        if parent.kind != EntryKind::Directory {
            return Err(OpfsError::TypeMismatch {
                path: name,
                expected: EntryKind::Directory,
                actual: parent.kind,
            });
        }
        let entry_id = self.take_entry_id()?;
        let backing_id = if kind == EntryKind::File {
            Some(self.take_backing_id()?)
        } else {
            None
        };
        let version_id = if kind == EntryKind::File {
            Some(self.take_version_id()?)
        } else {
            None
        };
        self.entries.insert(
            entry_id,
            CatalogEntry {
                parent_id,
                name,
                kind,
                backing_id,
                version_id,
                size: 0,
                modified_ms: now_ms,
            },
        );
        Ok((entry_id, backing_id))
    }

    pub(crate) fn replace_file_content(
        &mut self,
        entry_id: u64,
        size: u64,
        modified_ms: u64,
    ) -> OpfsResult<(u64, u64)> {
        let new_backing_id = self.take_backing_id()?;
        let new_version_id = self.take_version_id()?;
        let entry = self.entry_mut(entry_id)?;
        if entry.kind != EntryKind::File {
            return Err(OpfsError::TypeMismatch {
                path: entry.name.clone(),
                expected: EntryKind::File,
                actual: entry.kind,
            });
        }
        let old_backing_id = entry.backing_id.replace(new_backing_id).ok_or_else(|| {
            OpfsError::CorruptCatalog(format!("file entry {entry_id} has no backing id"))
        })?;
        entry.version_id = Some(new_version_id);
        entry.size = size;
        entry.modified_ms = modified_ms;
        Ok((old_backing_id, new_backing_id))
    }

    /// Record a new observable version while retaining the file's live backing.
    pub(crate) fn record_in_place_mutation(
        &mut self,
        entry_id: u64,
        size: u64,
        modified_ms: u64,
    ) -> OpfsResult<u64> {
        let version_id = self.take_version_id()?;
        self.record_reserved_in_place_mutation(entry_id, version_id, size, modified_ms)?;
        Ok(version_id)
    }

    pub(crate) fn record_reserved_in_place_mutation(
        &mut self,
        entry_id: u64,
        version_id: u64,
        size: u64,
        modified_ms: u64,
    ) -> OpfsResult<()> {
        if version_id == 0 || version_id >= self.next_version_id {
            return Err(OpfsError::CorruptCatalog(format!(
                "version ID {version_id} was not reserved by this catalog"
            )));
        }
        let entry = self.entry_mut(entry_id)?;
        if entry.kind != EntryKind::File {
            return Err(OpfsError::TypeMismatch {
                path: entry.name.clone(),
                expected: EntryKind::File,
                actual: entry.kind,
            });
        }
        if entry.backing_id.is_none() {
            return Err(OpfsError::CorruptCatalog(format!(
                "file entry {entry_id} has no backing id"
            )));
        }
        entry.version_id = Some(version_id);
        entry.size = size;
        entry.modified_ms = modified_ms;
        Ok(())
    }

    pub(crate) fn reserve_version_ids(&mut self, count: u64) -> OpfsResult<(u64, u64)> {
        if count == 0 {
            return Err(OpfsError::CorruptCatalog(
                "cannot reserve an empty version ID range".to_owned(),
            ));
        }
        let start = self.next_version_id.max(1);
        let end = start
            .checked_add(count)
            .ok_or_else(|| OpfsError::CorruptCatalog("version ID space is exhausted".to_owned()))?;
        self.next_version_id = end;
        Ok((start, end))
    }

    pub(crate) fn subtree_ids(&self, root_id: u64) -> Vec<u64> {
        let mut pending = vec![root_id];
        let mut result = Vec::new();
        while let Some(id) = pending.pop() {
            result.push(id);
            pending.extend(self.children(id).into_iter().map(|(child_id, _)| child_id));
        }
        result
    }

    pub(crate) fn remove_subtree(&mut self, root_id: u64) -> OpfsResult<Vec<u64>> {
        if root_id == ROOT_ENTRY_ID {
            return Err(OpfsError::InvalidModification(
                "the OPFS root cannot be removed".to_owned(),
            ));
        }
        let ids = self.subtree_ids(root_id);
        let mut backing_ids = Vec::new();
        for id in ids.into_iter().rev() {
            let entry = self.entries.remove(&id).ok_or_else(|| {
                OpfsError::CorruptCatalog(format!("missing subtree entry id {id}"))
            })?;
            if let Some(backing_id) = entry.backing_id {
                backing_ids.push(backing_id);
            }
        }
        Ok(backing_ids)
    }

    pub(crate) fn usage(&self) -> u64 {
        self.entries
            .iter()
            .filter(|(id, _)| **id != ROOT_ENTRY_ID)
            .map(|(_, entry)| {
                entry
                    .size
                    .saturating_add(PATH_CREATION_QUOTA_COST)
                    .saturating_add(
                        u64::try_from(entry.name.len())
                            .unwrap_or(u64::MAX)
                            .saturating_mul(PATH_BYTE_QUOTA_COST),
                    )
            })
            .fold(0u64, u64::saturating_add)
    }

    pub(crate) fn backing_ids(&self) -> Vec<u64> {
        self.entries
            .values()
            .filter_map(|entry| entry.backing_id)
            .collect()
    }

    pub(crate) fn validate(&self) -> OpfsResult<()> {
        let root = self
            .entries
            .get(&ROOT_ENTRY_ID)
            .ok_or_else(|| OpfsError::CorruptCatalog("catalog has no root entry".to_owned()))?;
        if root.parent_id != ROOT_ENTRY_ID
            || !root.name.is_empty()
            || root.kind != EntryKind::Directory
            || root.backing_id.is_some()
            || root.version_id.is_some()
            || root.size != 0
        {
            return Err(OpfsError::CorruptCatalog(
                "catalog root entry has invalid shape".to_owned(),
            ));
        }

        let mut siblings = BTreeSet::new();
        let mut backing_ids = BTreeSet::new();
        let mut version_ids = BTreeSet::new();
        for (id, entry) in &self.entries {
            if *id == ROOT_ENTRY_ID {
                continue;
            }
            validate_name(&entry.name)
                .map_err(|_| OpfsError::CorruptCatalog(format!("entry {id} has invalid name")))?;
            let parent = self.entries.get(&entry.parent_id).ok_or_else(|| {
                OpfsError::CorruptCatalog(format!(
                    "entry {id} references missing parent {}",
                    entry.parent_id
                ))
            })?;
            if parent.kind != EntryKind::Directory {
                return Err(OpfsError::CorruptCatalog(format!(
                    "entry {id} parent {} is not a directory",
                    entry.parent_id
                )));
            }
            if !siblings.insert((entry.parent_id, entry.name.clone())) {
                return Err(OpfsError::CorruptCatalog(format!(
                    "duplicate sibling name `{}` under parent {}",
                    entry.name, entry.parent_id
                )));
            }
            match (entry.kind, entry.backing_id, entry.version_id) {
                (EntryKind::Directory, None, None) => {
                    if entry.size != 0 {
                        return Err(OpfsError::CorruptCatalog(format!(
                            "directory entry {id} has non-zero size"
                        )));
                    }
                }
                (EntryKind::File, Some(backing_id), Some(version_id)) => {
                    if !backing_ids.insert(backing_id) {
                        return Err(OpfsError::CorruptCatalog(format!(
                            "duplicate backing id {backing_id}"
                        )));
                    }
                    if !version_ids.insert(version_id) {
                        return Err(OpfsError::CorruptCatalog(format!(
                            "duplicate version id {version_id}"
                        )));
                    }
                }
                _ => {
                    return Err(OpfsError::CorruptCatalog(format!(
                        "entry {id} kind/backing/version shape is invalid"
                    )));
                }
            }
            self.validate_parent_chain(*id)?;
        }
        let max_entry_id = self.entries.keys().copied().max().unwrap_or(ROOT_ENTRY_ID);
        if self.next_entry_id <= max_entry_id {
            return Err(OpfsError::CorruptCatalog(format!(
                "next entry id {} does not follow existing id {max_entry_id}",
                self.next_entry_id
            )));
        }
        let max_backing_id = backing_ids.iter().copied().max().unwrap_or(0);
        if self.next_backing_id <= max_backing_id {
            return Err(OpfsError::CorruptCatalog(format!(
                "next backing id {} does not follow existing id {max_backing_id}",
                self.next_backing_id
            )));
        }
        let max_version_id = version_ids.iter().copied().max().unwrap_or(0);
        if self.next_version_id <= max_version_id {
            return Err(OpfsError::CorruptCatalog(format!(
                "next version id {} does not follow existing id {max_version_id}",
                self.next_version_id
            )));
        }
        Ok(())
    }

    fn validate_parent_chain(&self, start_id: u64) -> OpfsResult<()> {
        let mut seen = BTreeSet::new();
        let mut current = start_id;
        while current != ROOT_ENTRY_ID {
            if !seen.insert(current) {
                return Err(OpfsError::CorruptCatalog(format!(
                    "entry {start_id} has a parent cycle"
                )));
            }
            current = self.entry(current)?.parent_id;
        }
        Ok(())
    }

    fn take_entry_id(&mut self) -> OpfsResult<u64> {
        let id = self.next_entry_id.max(1);
        self.next_entry_id = id
            .checked_add(1)
            .ok_or_else(|| OpfsError::CorruptCatalog("entry ID space is exhausted".to_owned()))?;
        Ok(id)
    }

    fn take_backing_id(&mut self) -> OpfsResult<u64> {
        let id = self.next_backing_id.max(1);
        self.next_backing_id = id
            .checked_add(1)
            .ok_or_else(|| OpfsError::CorruptCatalog("backing ID space is exhausted".to_owned()))?;
        Ok(id)
    }

    fn take_version_id(&mut self) -> OpfsResult<u64> {
        let id = self.next_version_id.max(1);
        self.next_version_id = id
            .checked_add(1)
            .ok_or_else(|| OpfsError::CorruptCatalog("version ID space is exhausted".to_owned()))?;
        Ok(id)
    }
}

#[cfg(test)]
mod tests {
    use super::Catalog;
    use crate::{EntryKind, OpfsError};

    #[test]
    fn validation_rejects_stale_allocators_that_could_reuse_ids() {
        let mut catalog = Catalog::new(1);
        let root = catalog.resolve(&Default::default()).unwrap();
        catalog
            .create_entry(root, "file".to_owned(), EntryKind::File, 2)
            .unwrap();

        catalog.next_entry_id = 1;
        assert!(matches!(
            catalog.validate(),
            Err(OpfsError::CorruptCatalog(_))
        ));

        catalog.next_entry_id = 2;
        catalog.next_backing_id = 1;
        assert!(matches!(
            catalog.validate(),
            Err(OpfsError::CorruptCatalog(_))
        ));

        catalog.next_backing_id = 2;
        catalog.next_version_id = 1;
        assert!(matches!(
            catalog.validate(),
            Err(OpfsError::CorruptCatalog(_))
        ));
    }

    #[test]
    fn typed_catalog_version_rejects_every_other_schema() {
        let catalog = Catalog::new(1);
        let mut json: serde_json::Value =
            serde_json::from_slice(&catalog.to_bytes().unwrap()).unwrap();
        assert_eq!(json["version"], 1);

        json["version"] = 2.into();
        assert!(matches!(
            Catalog::from_bytes(&serde_json::to_vec(&json).unwrap()),
            Err(OpfsError::CatalogJson(_))
        ));
    }

    #[test]
    fn reserved_sync_versions_are_never_reused_by_recovery_allocation() {
        let mut catalog = Catalog::new(1);
        let (entry_id, _) = catalog
            .create_entry(super::ROOT_ENTRY_ID, "file".to_owned(), EntryKind::File, 1)
            .unwrap();
        let (start, end) = catalog.reserve_version_ids(4).unwrap();
        assert_eq!(end - start, 4);
        catalog
            .record_reserved_in_place_mutation(entry_id, start, 1, 2)
            .unwrap();

        let recovered_version = catalog.record_in_place_mutation(entry_id, 2, 3).unwrap();
        assert_eq!(recovered_version, end);
        catalog.validate().unwrap();
        Catalog::from_bytes(&catalog.to_bytes().unwrap()).unwrap();
    }

    #[test]
    fn usage_includes_chromium_compatible_virtual_path_cost() {
        let mut catalog = Catalog::new(1);
        let root = catalog.resolve(&Default::default()).unwrap();
        let (directory, _) = catalog
            .create_entry(root, "目录".to_owned(), EntryKind::Directory, 2)
            .unwrap();
        let (file, _) = catalog
            .create_entry(directory, "a.txt".to_owned(), EntryKind::File, 3)
            .unwrap();
        catalog.replace_file_content(file, 4, 4).unwrap();

        assert_eq!(catalog.usage(), (146 + 2 * 6) + (146 + 2 * 5 + 4));
    }
}
