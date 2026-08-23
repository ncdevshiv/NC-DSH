use super::context_registry::RuntimeRealmRegistry;

pub(super) struct DocumentInspectorBackendState {
    pub(super) runtime_realms: RuntimeRealmRegistry,
}

impl DocumentInspectorBackendState {
    pub(super) fn new() -> Self {
        Self {
            runtime_realms: RuntimeRealmRegistry::new(),
        }
    }
}
