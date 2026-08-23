use std::{fmt, sync::Arc};

use moli_module_script_tree as module_tree;

use crate::document_module_graph::{
    ModuleMapKey, NativeModuleMapSingleModuleClient, NativeModulepreloadLinkClient,
};
use crate::frame_owner_model::FrameDocumentParserRootTerminalClient;

#[derive(Debug)]
pub(crate) enum ModuleMapFetchClient {
    SingleModuleFetch(NativeModuleMapSingleModuleClient),
    ParserRootModule(Box<FrameDocumentParserRootTerminalClient>),
    ModulePreloadLink(Arc<NativeModulepreloadLinkClient>),
}

impl ModuleMapFetchClient {
    pub(crate) fn single_module_fetch(client: NativeModuleMapSingleModuleClient) -> Self {
        Self::SingleModuleFetch(client)
    }

    pub(crate) fn parser_root_module(client: FrameDocumentParserRootTerminalClient) -> Self {
        Self::ParserRootModule(Box::new(client))
    }

    pub(crate) fn modulepreload_link(client: Arc<NativeModulepreloadLinkClient>) -> Self {
        Self::ModulePreloadLink(client)
    }

    pub(crate) fn is_single_module_fetch(&self) -> bool {
        matches!(self, Self::SingleModuleFetch(_))
    }

    pub(crate) fn is_module_script(&self) -> bool {
        matches!(
            self,
            Self::SingleModuleFetch(client)
                if client.is_module_script_client()
        )
    }

    pub(crate) fn is_modulepreload_link(&self) -> bool {
        matches!(self, Self::ModulePreloadLink(_))
    }

    pub(crate) fn is_parser_root_module(&self) -> bool {
        matches!(self, Self::ParserRootModule(_))
    }

    pub(crate) fn is_dynamic_import(&self) -> bool {
        matches!(
            self,
            Self::SingleModuleFetch(client) if client.is_dynamic_import_client()
        )
    }

    pub(crate) fn detach_single_module_client(
        &self,
        client: module_tree::SingleModuleClientToken,
    ) -> bool {
        matches!(
            self,
            Self::SingleModuleFetch(fetch_client)
                if fetch_client.token() == client
        )
    }
}

#[derive(Default)]
pub(crate) struct ModuleMapTerminalClients {
    single_module_fetch_clients: Vec<NativeModuleMapSingleModuleClient>,
    parser_root_module_clients: Vec<FrameDocumentParserRootTerminalClient>,
    modulepreload_link_clients: Vec<Arc<NativeModulepreloadLinkClient>>,
}

impl ModuleMapTerminalClients {
    pub(crate) fn push(&mut self, client: ModuleMapFetchClient) {
        match client {
            ModuleMapFetchClient::SingleModuleFetch(client) => {
                self.single_module_fetch_clients.push(client);
            }
            ModuleMapFetchClient::ParserRootModule(client) => {
                self.parser_root_module_clients.push(*client);
            }
            ModuleMapFetchClient::ModulePreloadLink(client) => {
                self.modulepreload_link_clients.push(client);
            }
        }
    }

    pub(crate) fn into_parts(
        self,
    ) -> (
        Vec<NativeModuleMapSingleModuleClient>,
        Vec<FrameDocumentParserRootTerminalClient>,
        Vec<Arc<NativeModulepreloadLinkClient>>,
    ) {
        (
            self.single_module_fetch_clients,
            self.parser_root_module_clients,
            self.modulepreload_link_clients,
        )
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.single_module_fetch_clients.is_empty()
            && self.parser_root_module_clients.is_empty()
            && self.modulepreload_link_clients.is_empty()
    }

    pub(crate) fn single_module_fetch_client_count(&self) -> usize {
        self.single_module_fetch_clients.len()
    }

    pub(crate) fn modulepreload_link_client_count(&self) -> usize {
        self.modulepreload_link_clients.len()
    }

    pub(crate) fn parser_root_module_client_count(&self) -> usize {
        self.parser_root_module_clients.len()
    }

    pub(crate) fn detach_single_module_client(
        &mut self,
        client: module_tree::SingleModuleClientToken,
    ) -> bool {
        let Some(position) = self
            .single_module_fetch_clients
            .iter()
            .position(|current| current.token() == client)
        else {
            return false;
        };
        self.single_module_fetch_clients.remove(position);
        true
    }

    pub(crate) fn retain_dynamic_import_clients(&mut self) {
        self.single_module_fetch_clients
            .retain(NativeModuleMapSingleModuleClient::is_dynamic_import_client);
        self.parser_root_module_clients.clear();
        self.modulepreload_link_clients.clear();
    }
}

pub(crate) struct ModuleMapTerminalNotification {
    key: ModuleMapKey,
    clients: ModuleMapTerminalClients,
    successful: bool,
}

impl fmt::Debug for ModuleMapTerminalNotification {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ModuleMapTerminalNotification")
            .field("key", &self.key)
            .field("successful", &self.successful)
            .field(
                "single_module_fetch_client_count",
                &self.clients.single_module_fetch_client_count(),
            )
            .field(
                "modulepreload_link_client_count",
                &self.clients.modulepreload_link_client_count(),
            )
            .field(
                "parser_root_module_client_count",
                &self.clients.parser_root_module_client_count(),
            )
            .finish()
    }
}

impl ModuleMapTerminalNotification {
    pub(crate) fn new(
        key: ModuleMapKey,
        clients: ModuleMapTerminalClients,
        successful: bool,
    ) -> Self {
        Self {
            key,
            clients,
            successful,
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.clients.is_empty()
    }

    pub(crate) fn detach_single_module_client(
        &mut self,
        client: module_tree::SingleModuleClientToken,
    ) -> bool {
        self.clients.detach_single_module_client(client)
    }

    pub(crate) fn retain_dynamic_import_clients(&mut self) {
        self.clients.retain_dynamic_import_clients();
    }

    pub(crate) fn into_parts(self) -> (ModuleMapKey, ModuleMapTerminalClients, bool) {
        (self.key, self.clients, self.successful)
    }
}
