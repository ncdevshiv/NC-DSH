use super::NativeDomBridge;

impl NativeDomBridge {
    pub(crate) fn collection_wrapper_template(&mut self) -> &v8::ObjectTemplate {
        self.bindings.collection_wrapper_template()
    }

    pub(crate) fn static_handle_node_list_wrapper_template(&mut self) -> &v8::ObjectTemplate {
        self.bindings.static_handle_node_list_wrapper_template()
    }

    pub(crate) fn live_collection_wrapper_template(&mut self) -> &v8::ObjectTemplate {
        self.bindings.live_collection_wrapper_template()
    }

    pub(crate) fn named_node_map_wrapper_template(&mut self) -> &v8::ObjectTemplate {
        self.bindings.named_node_map_wrapper_template()
    }

    pub(crate) fn node_iterator_wrapper_template(&mut self) -> &v8::ObjectTemplate {
        self.bindings.node_iterator_wrapper_template()
    }

    pub(crate) fn tree_walker_wrapper_template(&mut self) -> &v8::ObjectTemplate {
        self.bindings.tree_walker_wrapper_template()
    }
}
