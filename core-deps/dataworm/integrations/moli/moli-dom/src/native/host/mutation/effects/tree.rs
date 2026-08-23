use super::{DomHandle, DomMutationEffects};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DomTreeMutationEffects {
    pub(super) connected_roots: Vec<DomHandle>,
    pub(super) disconnected_roots: Vec<DomHandle>,
    pub(super) removed_open_popovers: Vec<DomHandle>,
}

impl DomTreeMutationEffects {
    pub fn connected_roots(&self) -> &[DomHandle] {
        &self.connected_roots
    }

    pub fn disconnected_roots(&self) -> &[DomHandle] {
        &self.disconnected_roots
    }

    pub fn removed_open_popovers(&self) -> &[DomHandle] {
        &self.removed_open_popovers
    }

    fn mark_removed_open_popover(&mut self, handle: DomHandle) {
        if !self.removed_open_popovers.contains(&handle) {
            self.removed_open_popovers.push(handle);
        }
    }

    pub(super) fn merge(&mut self, other: Self) {
        self.connected_roots.extend(other.connected_roots);
        self.disconnected_roots.extend(other.disconnected_roots);
        for popover in other.removed_open_popovers {
            self.mark_removed_open_popover(popover);
        }
    }
}

impl DomMutationEffects {
    pub(in crate::native::host::mutation) fn mark_connected_root(&mut self, handle: DomHandle) {
        self.changed = true;
        self.tree.connected_roots.push(handle);
    }

    pub(in crate::native::host::mutation) fn mark_disconnected_root(&mut self, handle: DomHandle) {
        self.changed = true;
        self.tree.disconnected_roots.push(handle);
    }

    pub(in crate::native::host::mutation) fn mark_removed_open_popover(
        &mut self,
        handle: DomHandle,
    ) {
        self.changed = true;
        self.tree.mark_removed_open_popover(handle);
    }

    /// Removes low-level disconnect markers for roots that were detached only
    /// as an intermediate step of one public remove-and-reinsert operation.
    /// Higher-level insertion plans own the observable moved-node lifecycle.
    pub fn suppress_intermediate_disconnections(&mut self, roots: &[DomHandle]) {
        self.tree
            .disconnected_roots
            .retain(|root| !roots.contains(root));
    }
}
