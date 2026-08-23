use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use moli_module_script_tree as module_tree;

use super::{
    ModuleCompiledRecordId, ModuleEntryId, ModuleFetchMetadata, ModuleIdentityHash,
    ModuleLoadError, ModuleMapEntry, ModuleMapEntryState, ModuleMapFetchClient,
    ModuleMapFetchDisposition, ModuleMapKey, ModuleMapTerminalNotification,
    ModuleResolvedDependency, ModuleSource, NativeModuleMapSingleModuleClient,
    NativeModulepreloadLinkClient,
};
use crate::frame_owner_model::FrameDocumentParserRootTerminalClient;

#[derive(Debug, Default)]
pub(crate) struct DocumentModuleMapCore {
    entries: Vec<ModuleMapEntry>,
    entries_by_key: HashMap<ModuleMapKey, ModuleEntryId>,
    pending_terminal_notifications: VecDeque<ModuleMapTerminalNotification>,
}

impl DocumentModuleMapCore {
    /// Drops clients owned by the replaced parser/Document while retaining
    /// the module map and dynamic-import joins owned by the live ScriptState.
    ///
    /// Blink stores its `Modulator` in `V8PerContextData`; `document.open()`
    /// replaces parser state without replacing that V8 context. Keeping the
    /// map also matters for an in-flight dynamic graph whose remaining fetches
    /// may join entries populated before the replacement.
    pub(crate) fn retain_script_state_clients_for_document_replacement(&mut self) {
        for entry in &mut self.entries {
            entry.retain_dynamic_import_clients();
        }
        for notification in &mut self.pending_terminal_notifications {
            notification.retain_dynamic_import_clients();
        }
        self.pending_terminal_notifications
            .retain(|notification| !notification.is_empty());
    }

    pub(crate) fn start_or_join_fetch(&mut self, key: ModuleMapKey) -> ModuleMapFetchDisposition {
        if let Some(entry_id) = self.entry_id(&key) {
            return match self.entry(entry_id).state() {
                ModuleMapEntryState::Fetching => {
                    ModuleMapFetchDisposition::JoinedFetching(entry_id)
                }
                ModuleMapEntryState::Fetched => ModuleMapFetchDisposition::AlreadyFetched(entry_id),
                ModuleMapEntryState::Compiled
                | ModuleMapEntryState::Instantiated
                | ModuleMapEntryState::Evaluating
                | ModuleMapEntryState::Evaluated => {
                    ModuleMapFetchDisposition::AlreadyCompiled(entry_id)
                }
                ModuleMapEntryState::Failed => ModuleMapFetchDisposition::AlreadyFailed(entry_id),
            };
        }
        let entry_id = self.entry_id_or_insert(key, ModuleMapEntryState::Fetching);
        ModuleMapFetchDisposition::StartedFetch(entry_id)
    }

    pub(crate) fn add_single_module_fetch_client(
        &mut self,
        key: ModuleMapKey,
        client: NativeModuleMapSingleModuleClient,
    ) {
        debug_assert!(
            self.entry_id(&key).is_some(),
            "module graph clients should join an existing module map entry"
        );
        let entry_id = self.entry_id_or_insert(key, ModuleMapEntryState::Fetching);
        self.entry_mut(entry_id)
            .push_fetch_client(ModuleMapFetchClient::single_module_fetch(client));
    }

    pub(crate) fn detach_single_module_fetch_client(
        &mut self,
        client: module_tree::SingleModuleClientToken,
    ) -> bool {
        let mut detached = false;
        for entry in &mut self.entries {
            detached |= entry.detach_single_module_fetch_client(client);
        }
        for notification in &mut self.pending_terminal_notifications {
            detached |= notification.detach_single_module_client(client);
        }
        self.pending_terminal_notifications
            .retain(|notification| !notification.is_empty());
        detached
    }

    pub(crate) fn take_next_terminal_notification(
        &mut self,
    ) -> Option<ModuleMapTerminalNotification> {
        while let Some(notification) = self.pending_terminal_notifications.pop_front() {
            if !notification.is_empty() {
                return Some(notification);
            }
        }
        None
    }

    pub(crate) fn add_modulepreload_link_client(
        &mut self,
        key: ModuleMapKey,
        client: Arc<NativeModulepreloadLinkClient>,
    ) {
        debug_assert!(
            self.entry_id(&key).is_some(),
            "modulepreload link clients should join an existing module map entry"
        );
        debug_assert_eq!(
            client.key(),
            &key,
            "modulepreload client must stay attached to its captured module key"
        );
        let entry_id = self.entry_id_or_insert(key, ModuleMapEntryState::Fetching);
        self.entry_mut(entry_id)
            .push_fetch_client(ModuleMapFetchClient::modulepreload_link(client));
    }

    pub(crate) fn add_terminal_modulepreload_link_client(
        &mut self,
        key: ModuleMapKey,
        client: Arc<NativeModulepreloadLinkClient>,
    ) {
        debug_assert!(
            self.entry_id(&key).is_some(),
            "modulepreload link clients should join an existing module map entry"
        );
        debug_assert_eq!(
            client.key(),
            &key,
            "modulepreload client must stay attached to its captured module key"
        );
        let entry_id = self.entry_id_or_insert(key, ModuleMapEntryState::Fetching);
        self.entry_mut(entry_id)
            .push_fetch_client(ModuleMapFetchClient::modulepreload_link(client));
        self.enqueue_terminal_notification_if_entry_terminal(entry_id);
    }

    pub(crate) fn add_parser_root_module_client(
        &mut self,
        key: ModuleMapKey,
        client: FrameDocumentParserRootTerminalClient,
    ) {
        debug_assert!(
            self.entry_id(&key).is_some(),
            "parser root module clients should join an existing module map entry"
        );
        let entry_id = self.entry_id_or_insert(key, ModuleMapEntryState::Fetching);
        self.entry_mut(entry_id)
            .push_fetch_client(ModuleMapFetchClient::parser_root_module(client));
        self.enqueue_terminal_notification_if_entry_terminal(entry_id);
    }

    #[cfg(test)]
    pub(crate) fn fetch_client_count(&self) -> usize {
        self.entries
            .iter()
            .map(ModuleMapEntry::fetch_client_count)
            .sum()
    }

    #[cfg(test)]
    pub(crate) fn single_module_fetch_client_count(&self) -> usize {
        self.entries
            .iter()
            .map(ModuleMapEntry::single_module_fetch_client_count)
            .sum()
    }

    #[cfg(test)]
    pub(crate) fn module_script_client_count(&self) -> usize {
        self.entries
            .iter()
            .map(ModuleMapEntry::module_script_client_count)
            .sum()
    }

    #[cfg(test)]
    pub(crate) fn modulepreload_link_client_count(&self) -> usize {
        self.entries
            .iter()
            .map(ModuleMapEntry::modulepreload_link_client_count)
            .sum()
    }

    #[cfg(test)]
    pub(crate) fn parser_root_module_client_count(&self) -> usize {
        self.entries
            .iter()
            .map(ModuleMapEntry::parser_root_module_client_count)
            .sum()
    }

    #[cfg(test)]
    pub(crate) fn insert_fetched_source(
        &mut self,
        key: ModuleMapKey,
        source: ModuleSource,
    ) -> (
        ModuleEntryId,
        Option<(ModuleCompiledRecordId, Option<ModuleIdentityHash>)>,
    ) {
        let entry_id = self.entry_id_or_insert(key.clone(), ModuleMapEntryState::Fetched);
        let cleared = self.clear_compiled_record(entry_id);
        self.entry_mut(entry_id).set_fetched_source(
            key.clone(),
            key,
            source,
            ModuleFetchMetadata::default(),
        );
        self.enqueue_terminal_notification_if_needed(entry_id);
        (entry_id, cleared)
    }

    pub(crate) fn insert_fetched_source_for_request(
        &mut self,
        request_key: ModuleMapKey,
        effective_key: ModuleMapKey,
        source: ModuleSource,
        effective_fetch_metadata: ModuleFetchMetadata,
    ) -> (
        ModuleEntryId,
        Option<(ModuleCompiledRecordId, Option<ModuleIdentityHash>)>,
    ) {
        let entry_id = self.entry_id_or_insert(request_key.clone(), ModuleMapEntryState::Fetched);
        let cleared = self.clear_compiled_record(entry_id);
        self.entry_mut(entry_id).set_fetched_source(
            request_key,
            effective_key,
            source,
            effective_fetch_metadata,
        );
        self.enqueue_terminal_notification_if_needed(entry_id);
        (entry_id, cleared)
    }

    pub(crate) fn entry_id(&self, key: &ModuleMapKey) -> Option<ModuleEntryId> {
        self.entries_by_key.get(key).copied()
    }

    pub(crate) fn mark_failed(
        &mut self,
        key: ModuleMapKey,
        error: ModuleLoadError,
    ) -> (
        ModuleEntryId,
        Option<(ModuleCompiledRecordId, Option<ModuleIdentityHash>)>,
    ) {
        let entry_id = self.entry_id_or_insert(key.clone(), ModuleMapEntryState::Failed);
        let cleared = self.clear_compiled_record(entry_id);
        self.entry_mut(entry_id).set_failed(key, error);
        self.enqueue_terminal_notification_if_needed(entry_id);
        (entry_id, cleared)
    }

    pub(crate) fn insert_compiled_record_with_metadata(
        &mut self,
        request_key: ModuleMapKey,
        effective_key: ModuleMapKey,
        record_id: ModuleCompiledRecordId,
        identity: ModuleIdentityHash,
        effective_fetch_metadata: ModuleFetchMetadata,
    ) -> (
        ModuleEntryId,
        Option<(ModuleCompiledRecordId, Option<ModuleIdentityHash>)>,
    ) {
        // Keep the entry indexed by the pre-redirect request key. Dependency
        // resolution produces request keys for lookups; effective_key is the
        // post-redirect module URL used for metadata, referrer, and
        // import.meta.url-style observations.
        let entry_id = self.entry_id_or_insert(request_key.clone(), ModuleMapEntryState::Compiled);
        let cleared = self.clear_compiled_record(entry_id);
        self.entry_mut(entry_id).set_compiled_record(
            request_key,
            effective_key,
            record_id,
            identity,
            effective_fetch_metadata,
        );
        (entry_id, cleared)
    }

    pub(crate) fn set_resolved_dependencies(
        &mut self,
        entry_id: ModuleEntryId,
        dependencies: Vec<ModuleResolvedDependency>,
    ) {
        self.entry_mut(entry_id)
            .set_resolved_dependencies(dependencies);
    }

    #[cfg(test)]
    pub(crate) fn resolved_dependencies(
        &self,
        entry_id: ModuleEntryId,
    ) -> &[ModuleResolvedDependency] {
        self.entry(entry_id).resolved_dependencies()
    }

    pub(crate) fn mark_instantiated(&mut self, entry_id: ModuleEntryId) {
        self.entry_mut(entry_id).mark_instantiated();
    }

    pub(crate) fn mark_evaluating(&mut self, entry_id: ModuleEntryId) {
        self.entry_mut(entry_id).mark_evaluating();
    }

    pub(crate) fn mark_evaluated(&mut self, entry_id: ModuleEntryId) {
        self.entry_mut(entry_id).mark_evaluated();
    }

    pub(crate) fn entry(&self, entry_id: ModuleEntryId) -> &ModuleMapEntry {
        &self.entries[entry_id.index()]
    }

    pub(crate) fn entries(&self) -> &[ModuleMapEntry] {
        &self.entries
    }

    fn entry_mut(&mut self, entry_id: ModuleEntryId) -> &mut ModuleMapEntry {
        &mut self.entries[entry_id.index()]
    }

    pub(crate) fn clear_compiled_record(
        &mut self,
        entry_id: ModuleEntryId,
    ) -> Option<(ModuleCompiledRecordId, Option<ModuleIdentityHash>)> {
        self.entry_mut(entry_id).clear_compiled_record()
    }

    fn enqueue_terminal_notification_if_needed(&mut self, entry_id: ModuleEntryId) {
        let successful = self.entry(entry_id).state() != ModuleMapEntryState::Failed;
        let (key, clients) = {
            let entry = self.entry_mut(entry_id);
            (entry.key().clone(), entry.take_terminal_clients())
        };
        if clients.is_empty() {
            return;
        }
        self.pending_terminal_notifications
            .push_back(ModuleMapTerminalNotification::new(key, clients, successful));
    }

    fn enqueue_terminal_notification_if_entry_terminal(&mut self, entry_id: ModuleEntryId) {
        match self.entry(entry_id).state() {
            ModuleMapEntryState::Fetching => {}
            ModuleMapEntryState::Fetched
            | ModuleMapEntryState::Compiled
            | ModuleMapEntryState::Instantiated
            | ModuleMapEntryState::Evaluating
            | ModuleMapEntryState::Evaluated
            | ModuleMapEntryState::Failed => self.enqueue_terminal_notification_if_needed(entry_id),
        }
    }

    fn entry_id_or_insert(
        &mut self,
        key: ModuleMapKey,
        state: ModuleMapEntryState,
    ) -> ModuleEntryId {
        if let Some(entry_id) = self.entries_by_key.get(&key) {
            return *entry_id;
        }
        let entry_id = ModuleEntryId::from_index(self.entries.len());
        self.entries.push(ModuleMapEntry::new(key.clone(), state));
        self.entries_by_key.insert(key, entry_id);
        entry_id
    }
}
