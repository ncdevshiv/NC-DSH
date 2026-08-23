use super::{DomHandle, DomMutationEffects};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DomScriptMutationEffects {
    pub(super) connected_roots: Vec<DomHandle>,
    pub(super) updated_nodes: Vec<DomHandle>,
    pub(super) prepare_triggers: Vec<ScriptPrepareTrigger>,
}

impl DomScriptMutationEffects {
    pub fn connected_roots(&self) -> &[DomHandle] {
        &self.connected_roots
    }

    pub fn updated_nodes(&self) -> &[DomHandle] {
        &self.updated_nodes
    }

    pub fn prepare_triggers(&self) -> &[ScriptPrepareTrigger] {
        &self.prepare_triggers
    }

    pub(super) fn merge(&mut self, other: Self) {
        self.connected_roots.extend(other.connected_roots);
        self.updated_nodes.extend(other.updated_nodes);
        for trigger in other.prepare_triggers {
            if !self.prepare_triggers.contains(&trigger) {
                self.prepare_triggers.push(trigger);
            }
            if !self.updated_nodes.contains(&trigger.handle) {
                self.updated_nodes.push(trigger.handle);
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScriptPrepareTrigger {
    pub(super) handle: DomHandle,
    pub(super) kind: ScriptPrepareTriggerKind,
}

impl ScriptPrepareTrigger {
    pub fn handle(self) -> DomHandle {
        self.handle
    }

    pub fn kind(self) -> ScriptPrepareTriggerKind {
        self.kind
    }

    pub fn clears_script_force_async(self) -> bool {
        matches!(self.kind, ScriptPrepareTriggerKind::AsyncAttributeAdded)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptPrepareTriggerKind {
    Connected,
    ChildInsertion,
    SourceAttributeAdded,
    AsyncAttributeAdded,
}

impl DomMutationEffects {
    pub(in crate::native::host::mutation) fn mark_connected_script_root(
        &mut self,
        handle: DomHandle,
    ) {
        self.changed = true;
        self.scripts.connected_roots.push(handle);
    }

    fn mark_updated_script_node(&mut self, handle: DomHandle) {
        self.changed = true;
        if !self.scripts.updated_nodes.contains(&handle) {
            self.scripts.updated_nodes.push(handle);
        }
    }

    pub(in crate::native::host::mutation) fn mark_script_prepare_trigger(
        &mut self,
        handle: DomHandle,
        kind: ScriptPrepareTriggerKind,
    ) {
        self.changed = true;
        let trigger = ScriptPrepareTrigger { handle, kind };
        if !self.scripts.prepare_triggers.contains(&trigger) {
            self.scripts.prepare_triggers.push(trigger);
        }
        self.mark_updated_script_node(handle);
    }
}
