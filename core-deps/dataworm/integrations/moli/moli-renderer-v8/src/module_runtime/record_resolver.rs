use std::collections::HashMap;

use crate::document_module_graph::{
    ModuleCompiledRecordId, ModuleEntryId, ModuleIdentityHash, ModuleMapEntry,
};

use super::record::{ModuleRecordEntry, ModuleRecordState};

#[derive(Debug, Default)]
pub(crate) struct NativeModuleRecordResolver {
    compiled_records: Vec<Option<ModuleRecordEntry>>,
    module_to_entries: HashMap<ModuleIdentityHash, Vec<ModuleEntryId>>,
}

impl NativeModuleRecordResolver {
    pub(crate) fn insert_record(&mut self, record: ModuleRecordEntry) -> ModuleCompiledRecordId {
        let record_id = ModuleCompiledRecordId::from_index(self.compiled_records.len());
        self.compiled_records.push(Some(record));
        record_id
    }

    pub(crate) fn register_entry_identity(
        &mut self,
        entry_id: ModuleEntryId,
        identity: ModuleIdentityHash,
    ) {
        let entries = self.module_to_entries.entry(identity).or_default();
        if !entries.contains(&entry_id) {
            entries.push(entry_id);
        }
    }

    pub(crate) fn detach_cleared_record(
        &mut self,
        entry_id: ModuleEntryId,
        cleared: Option<(ModuleCompiledRecordId, Option<ModuleIdentityHash>)>,
    ) {
        let Some((record_id, identity)) = cleared else {
            return;
        };
        if let Some(record) = self.compiled_records.get_mut(record_id.index()) {
            *record = None;
        }
        if let Some(identity) = identity
            && let Some(entries) = self.module_to_entries.get_mut(&identity)
        {
            entries.retain(|candidate| *candidate != entry_id);
            if entries.is_empty() {
                self.module_to_entries.remove(&identity);
            }
        }
    }

    pub(crate) fn entry_ids_for_identity(
        &self,
        identity: ModuleIdentityHash,
    ) -> Option<&[ModuleEntryId]> {
        Some(self.module_to_entries.get(&identity)?.as_slice())
    }

    pub(crate) fn compiled_record_by_id(
        &self,
        record_id: ModuleCompiledRecordId,
    ) -> Option<&ModuleRecordEntry> {
        self.compiled_records.get(record_id.index())?.as_ref()
    }

    pub(crate) fn compiled_record_mut_for_entry(
        &mut self,
        entry: &ModuleMapEntry,
    ) -> Option<&mut ModuleRecordEntry> {
        let record_id = entry.compiled_record_id()?;
        self.compiled_records.get_mut(record_id.index())?.as_mut()
    }

    pub(crate) fn set_record_state_for_entry(
        &mut self,
        entry: &ModuleMapEntry,
        state: ModuleRecordState,
    ) {
        if let Some(record) = self.compiled_record_mut_for_entry(entry) {
            record.set_state(state);
        }
    }

    pub(crate) fn record_for_entry(&self, entry: &ModuleMapEntry) -> Option<&ModuleRecordEntry> {
        self.compiled_record_by_id(entry.compiled_record_id()?)
    }

    pub(crate) fn usable_record_for_entry(
        &self,
        entry: &ModuleMapEntry,
    ) -> Option<&ModuleRecordEntry> {
        self.compiled_record_by_id(entry.usable_compiled_record_id()?)
    }
}
