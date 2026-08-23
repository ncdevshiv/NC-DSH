use moli_module_script_tree as module_tree;

use super::{ModuleEntryId, ModuleImportPhase, ModuleLoadError, ModuleLoadStage, ModuleMapKey};

pub(super) struct NativeModuleTreeJob {
    tree: module_tree::ModuleScriptTreeJob,
    pending_joined_clients: Vec<module_tree::SingleModuleClientToken>,
}

pub(super) enum NativeModuleTreeJobAdvance {
    NeedFetches(Vec<NativeModuleTreeFetchRequest>),
    WaitingForFetches { client_count: usize },
    Complete(module_tree::ModuleGraphHandle),
    Failed(module_tree::ModuleLoadError),
    Aborted(module_tree::ModuleTreeAbortReason),
    PendingWithoutWork,
    IgnoredStaleCompletion,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NativeModuleGraphDependencyRequest {
    parent_key: ModuleMapKey,
    parent_entry_id: ModuleEntryId,
    specifier: String,
    phase: ModuleImportPhase,
}

impl NativeModuleGraphDependencyRequest {
    pub(super) fn new(
        parent_key: ModuleMapKey,
        parent_entry_id: ModuleEntryId,
        specifier: String,
        phase: ModuleImportPhase,
    ) -> Self {
        Self {
            parent_key,
            parent_entry_id,
            specifier,
            phase,
        }
    }

    pub(crate) fn parent_key(&self) -> &ModuleMapKey {
        &self.parent_key
    }

    pub(crate) fn parent_entry_id(&self) -> ModuleEntryId {
        self.parent_entry_id
    }

    pub(crate) fn specifier(&self) -> &str {
        &self.specifier
    }

    pub(crate) fn phase(&self) -> ModuleImportPhase {
        self.phase
    }
}

pub(super) struct NativeModuleTreeFetchRequest {
    request: module_tree::ModuleFetchRequest,
    key: ModuleMapKey,
    dependency: Option<NativeModuleGraphDependencyRequest>,
}

impl NativeModuleTreeFetchRequest {
    pub(super) fn new(
        request: module_tree::ModuleFetchRequest,
        key: ModuleMapKey,
        dependency: Option<NativeModuleGraphDependencyRequest>,
    ) -> Self {
        Self {
            request,
            key,
            dependency,
        }
    }

    pub(super) fn request(&self) -> &module_tree::ModuleFetchRequest {
        &self.request
    }

    pub(super) fn key(&self) -> &ModuleMapKey {
        &self.key
    }

    pub(super) fn dependency(&self) -> Option<&NativeModuleGraphDependencyRequest> {
        self.dependency.as_ref()
    }

    pub(super) fn client(&self) -> module_tree::SingleModuleClientToken {
        self.request.client
    }

    pub(super) fn graph_level(&self) -> module_tree::ModuleGraphLevel {
        self.request.graph_level
    }
}

impl NativeModuleTreeJob {
    pub(super) fn new(tree: module_tree::ModuleScriptTreeJob) -> Self {
        Self {
            tree,
            pending_joined_clients: Vec::new(),
        }
    }

    pub(super) fn chromium_tree(&self) -> &module_tree::ModuleScriptTreeJob {
        &self.tree
    }

    pub(super) fn drive(
        &mut self,
        host: &mut impl module_tree::ModuleScriptTreeHost,
    ) -> module_tree::ModuleScriptTreeDrive {
        self.tree.drive(host)
    }

    pub(super) fn resume_single_module_outcome_and_drive(
        &mut self,
        host: &mut impl module_tree::ModuleScriptTreeHost,
        client: module_tree::SingleModuleClientToken,
        outcome: module_tree::ModuleFetchOutcome,
    ) -> module_tree::ModuleScriptTreeDrive {
        self.tree
            .resume_single_module_outcome_and_drive(host, client, outcome)
    }

    pub(super) fn take_pending_joined_clients(
        &mut self,
    ) -> Vec<module_tree::SingleModuleClientToken> {
        std::mem::take(&mut self.pending_joined_clients)
    }

    #[cfg(test)]
    pub(super) fn pending_joined_client_count(&self) -> usize {
        self.pending_joined_clients.len()
    }

    pub(super) fn absorb_joined_fetches(
        &mut self,
        joined_fetches: Vec<module_tree::ModuleFetchRequest>,
        mut validate_key: impl FnMut(&module_tree::ModuleMapKey) -> Result<(), ModuleLoadError>,
    ) -> Result<(), ModuleLoadError> {
        for request in joined_fetches {
            validate_key(&request.key)?;
            self.pending_joined_clients.push(request.client);
        }
        Ok(())
    }

    pub(super) fn advance_from_drive(
        &mut self,
        drive: module_tree::ModuleScriptTreeDrive,
        validate_key: impl FnMut(&module_tree::ModuleMapKey) -> Result<(), ModuleLoadError>,
        mut convert_fetch: impl FnMut(
            module_tree::ModuleFetchRequest,
        ) -> Result<NativeModuleTreeFetchRequest, ModuleLoadError>,
    ) -> Result<NativeModuleTreeJobAdvance, ModuleLoadError> {
        match drive {
            module_tree::ModuleScriptTreeDrive::NeedFetches(fetches) => {
                let (fetches, joined_fetches) = fetches.into_parts();
                self.absorb_joined_fetches(joined_fetches, validate_key)?;
                if fetches.is_empty() {
                    return Err(ModuleLoadError::new(
                        ModuleLoadStage::Fetch,
                        "module script tree requested an empty fetch batch",
                    ));
                }
                let mut native_fetches = Vec::with_capacity(fetches.len());
                for fetch in fetches {
                    native_fetches.push(convert_fetch(fetch)?);
                }
                Ok(NativeModuleTreeJobAdvance::NeedFetches(native_fetches))
            }
            module_tree::ModuleScriptTreeDrive::WaitingForSingleModuleClients(wait) => {
                debug_assert!(
                    wait.has_clients(),
                    "waiting module tree poll should include at least one pending client"
                );
                self.absorb_joined_fetches(wait.joined_fetches, validate_key)?;
                Ok(NativeModuleTreeJobAdvance::WaitingForFetches {
                    client_count: wait.client_count,
                })
            }
            module_tree::ModuleScriptTreeDrive::Complete(graph) => {
                Ok(NativeModuleTreeJobAdvance::Complete(graph))
            }
            module_tree::ModuleScriptTreeDrive::Failed(error) => {
                Ok(NativeModuleTreeJobAdvance::Failed(error))
            }
            module_tree::ModuleScriptTreeDrive::Aborted(reason) => {
                Ok(NativeModuleTreeJobAdvance::Aborted(reason))
            }
            module_tree::ModuleScriptTreeDrive::Pending(idle) => {
                if !idle.joined_fetches.is_empty() {
                    self.absorb_joined_fetches(idle.joined_fetches, validate_key)?;
                    return Ok(NativeModuleTreeJobAdvance::WaitingForFetches { client_count: 0 });
                }
                Ok(NativeModuleTreeJobAdvance::PendingWithoutWork)
            }
            module_tree::ModuleScriptTreeDrive::IgnoredStaleCompletion(_) => {
                Ok(NativeModuleTreeJobAdvance::IgnoredStaleCompletion)
            }
        }
    }
}
