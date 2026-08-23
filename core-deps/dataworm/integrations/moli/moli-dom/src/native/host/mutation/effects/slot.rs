use super::{DomHandle, DomMutationEffects};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DomSlotMutationEffects {
    pub(super) changed_slots: Vec<DomHandle>,
    pub(super) assignment_changes: Vec<DomSlotAssignmentChange>,
}

impl DomSlotMutationEffects {
    pub fn changed_slots(&self) -> &[DomHandle] {
        &self.changed_slots
    }

    pub fn assignment_changes(&self) -> &[DomSlotAssignmentChange] {
        &self.assignment_changes
    }

    fn mark_changed(&mut self, handle: DomHandle) {
        if !self.changed_slots.contains(&handle) {
            self.changed_slots.push(handle);
        }
    }

    fn mark_assignment_change(&mut self, change: DomSlotAssignmentChange) {
        if change.previous_assigned_nodes == change.assigned_nodes {
            return;
        }
        self.mark_changed(change.slot);
        if let Some(existing) = self
            .assignment_changes
            .iter_mut()
            .find(|existing| existing.slot == change.slot)
        {
            existing.assigned_nodes = change.assigned_nodes;
            return;
        }
        self.assignment_changes.push(change);
    }

    pub(super) fn merge(&mut self, other: Self) {
        for slot in other.changed_slots {
            self.mark_changed(slot);
        }
        for change in other.assignment_changes {
            self.mark_assignment_change(change);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DomSlotAssignmentChange {
    pub(super) slot: DomHandle,
    pub(super) previous_assigned_nodes: Vec<DomHandle>,
    pub(super) assigned_nodes: Vec<DomHandle>,
}

impl DomSlotAssignmentChange {
    pub(in crate::native::host) fn new(
        slot: DomHandle,
        previous_assigned_nodes: Vec<DomHandle>,
        assigned_nodes: Vec<DomHandle>,
    ) -> Self {
        Self {
            slot,
            previous_assigned_nodes,
            assigned_nodes,
        }
    }

    pub fn slot(&self) -> DomHandle {
        self.slot
    }

    pub fn previous_assigned_nodes(&self) -> &[DomHandle] {
        &self.previous_assigned_nodes
    }

    pub fn assigned_nodes(&self) -> &[DomHandle] {
        &self.assigned_nodes
    }
}

impl DomMutationEffects {
    pub(in crate::native::host) fn mark_changed_slot(&mut self, handle: DomHandle) {
        self.changed = true;
        self.slots.mark_changed(handle);
    }

    pub(in crate::native::host) fn mark_slot_assignment_change(
        &mut self,
        slot: DomHandle,
        previous_assigned_nodes: Vec<DomHandle>,
        assigned_nodes: Vec<DomHandle>,
    ) {
        let change = DomSlotAssignmentChange::new(slot, previous_assigned_nodes, assigned_nodes);
        if change.previous_assigned_nodes == change.assigned_nodes {
            return;
        }
        self.changed = true;
        self.slots.mark_assignment_change(change);
    }
}
