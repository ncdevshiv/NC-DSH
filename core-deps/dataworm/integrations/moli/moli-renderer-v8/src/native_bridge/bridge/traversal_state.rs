use super::super::traversal;
use super::super::traversal::TraversalFilter;
use super::NativeDomBridge;
use crate::document_runtime::DomHandle;

impl NativeDomBridge {
    pub(crate) fn node_iterators_is_empty(&self) -> bool {
        self.traversal.node_iterators_is_empty()
    }

    pub(in crate::native_bridge) fn register_node_iterator(
        &mut self,
        root: DomHandle,
        what_to_show: u32,
        filter: Option<TraversalFilter>,
    ) -> u32 {
        self.traversal
            .register_node_iterator(root, what_to_show, filter)
    }

    pub(in crate::native_bridge) fn register_tree_walker(
        &mut self,
        root: DomHandle,
        what_to_show: u32,
        filter: Option<TraversalFilter>,
    ) -> u32 {
        self.traversal
            .register_tree_walker(root, what_to_show, filter)
    }

    pub(in crate::native_bridge) fn node_iterator_snapshot(
        &self,
        scope: &mut v8::PinScope<'_, '_>,
        id: u32,
    ) -> Option<traversal::NodeIteratorSnapshot> {
        self.traversal.node_iterator_snapshot(scope, id)
    }

    pub(in crate::native_bridge) fn tree_walker_snapshot(
        &self,
        scope: &mut v8::PinScope<'_, '_>,
        id: u32,
    ) -> Option<traversal::TreeWalkerSnapshot> {
        self.traversal.tree_walker_snapshot(scope, id)
    }

    pub(crate) fn set_node_iterator_position(
        &mut self,
        id: u32,
        reference_node: DomHandle,
        pointer_before_reference_node: bool,
    ) {
        self.traversal.set_node_iterator_position(
            id,
            reference_node,
            pointer_before_reference_node,
        );
    }

    pub(crate) fn apply_node_iterator_pre_remove_steps<F>(
        &mut self,
        removed_node: DomHandle,
        previous_node: DomHandle,
        next_after_removed_subtree: Option<DomHandle>,
        is_inclusive_ancestor: F,
    ) where
        F: FnMut(DomHandle, DomHandle) -> bool,
    {
        self.traversal.apply_node_iterator_pre_remove_steps(
            removed_node,
            previous_node,
            next_after_removed_subtree,
            is_inclusive_ancestor,
        );
    }

    pub(crate) fn set_tree_walker_current_node(&mut self, id: u32, current_node: DomHandle) {
        self.traversal
            .set_tree_walker_current_node(id, current_node);
    }

    pub(crate) fn node_iterator_try_begin(&mut self, id: u32) -> bool {
        self.traversal.node_iterator_try_begin(id)
    }

    pub(crate) fn node_iterator_end(&mut self, id: u32) {
        self.traversal.node_iterator_end(id);
    }

    pub(crate) fn tree_walker_try_begin(&mut self, id: u32) -> bool {
        self.traversal.tree_walker_try_begin(id)
    }

    pub(crate) fn tree_walker_end(&mut self, id: u32) {
        self.traversal.tree_walker_end(id);
    }
}
