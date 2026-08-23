use crate::frame_owner_model::FrameDocumentModuleClientId;

#[derive(Debug)]
pub(crate) struct NativeDocumentModuleOwnerIds {
    next_module_graph_fetch_load_id: u64,
    next_parser_root_client_id: u64,
}

impl Default for NativeDocumentModuleOwnerIds {
    fn default() -> Self {
        Self {
            next_module_graph_fetch_load_id: 1,
            next_parser_root_client_id: 1,
        }
    }
}

impl NativeDocumentModuleOwnerIds {
    pub(crate) fn reserve_module_graph_fetch_load_id(&mut self) -> u64 {
        let load_id = self.next_module_graph_fetch_load_id;
        self.next_module_graph_fetch_load_id = self
            .next_module_graph_fetch_load_id
            .checked_add(1)
            .expect("document module graph fetch load id should not overflow");
        load_id
    }

    pub(crate) fn reserve_parser_root_module_client_id(&mut self) -> FrameDocumentModuleClientId {
        let client_id = self.next_parser_root_client_id;
        self.next_parser_root_client_id = self
            .next_parser_root_client_id
            .checked_add(1)
            .expect("document parser root module client id should not overflow");
        FrameDocumentModuleClientId::from_raw(client_id)
    }
}
