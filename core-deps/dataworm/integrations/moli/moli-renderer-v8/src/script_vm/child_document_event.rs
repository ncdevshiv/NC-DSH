use anyhow::{Result, anyhow};

use crate::{
    document_runtime::DomHandle,
    frame_owner_model::{
        FrameDocumentScriptElementEvent, FrameDocumentScriptElementEventKind,
        FrameDocumentTaskOwner, FrameRealmId,
    },
};

use super::ScriptVm;

pub(super) struct ChildDocumentEventOwner<'vm> {
    vm: &'vm mut ScriptVm,
}

impl<'vm> ChildDocumentEventOwner<'vm> {
    pub(super) fn new(vm: &'vm mut ScriptVm) -> Self {
        Self { vm }
    }

    pub(super) fn dispatch_script_element_event(
        &mut self,
        event: FrameDocumentScriptElementEvent,
    ) -> Result<()> {
        let realm_id = self
            .vm
            .current_child_frame_realm_id_for_document_owner(event.child_handle, event.owner)?;
        self.dispatch_script_element_event_in_realm_selected_task_body(realm_id, event)
    }

    /// Dispatch a child script element's terminal event inside an
    /// already-selected `DocumentScriptReady` task.
    ///
    /// Module evaluation may have performed its own algorithm-required
    /// error-handling checkpoint before this call. Neither that checkpoint nor
    /// this event body ends the enclosing HTML task; the selected Page-task
    /// dispatcher performs its sole task-end completion after this returns.
    pub(super) fn dispatch_script_element_event_for_parts_selected_task_body(
        &mut self,
        owner: FrameDocumentTaskOwner,
        realm_id: FrameRealmId,
        script_handle: DomHandle,
        kind: FrameDocumentScriptElementEventKind,
    ) -> Result<()> {
        let snapshot = self
            .vm
            ._context_host
            .borrow()
            .frame_owner_current_child_snapshot_for_realm(realm_id)
            .ok_or_else(|| anyhow!("child script element event has no current FrameRealm"))?;
        if snapshot.local_window_id != owner.local_window_id
            || snapshot.document_id != owner.document_id
        {
            return Err(anyhow!("child script element event owner token is stale"));
        }
        let event = FrameDocumentScriptElementEvent {
            child_handle: snapshot.owner_handle,
            owner: owner.document_owner(),
            script_handle,
            kind,
        };
        self.dispatch_script_element_event(event)
    }

    fn dispatch_script_element_event_in_realm_selected_task_body(
        &mut self,
        realm_id: FrameRealmId,
        event: FrameDocumentScriptElementEvent,
    ) -> Result<()> {
        let context_host = self.vm._context_host.clone();
        self.vm
            .with_frame_realm_scope(realm_id, move |scope, _host_ptr| {
                let dispatched = context_host
                    .borrow_mut()
                    .dispatch_child_script_element_event(scope, event);
                if !dispatched {
                    return Err(anyhow!(
                        "child script element event target {:?} is no longer current",
                        event.script_handle
                    ));
                }
                Ok(())
            })
    }

    pub(super) fn dispatch_modulepreload_link_handle_event(
        &mut self,
        owner: FrameDocumentTaskOwner,
        realm_id: FrameRealmId,
        link_handle: DomHandle,
        successful: bool,
    ) -> Result<()> {
        let context_host = self.vm._context_host.clone();
        self.vm
            .with_frame_realm_scope(realm_id, move |scope, _host_ptr| {
                let dispatched = context_host
                    .borrow_mut()
                    .dispatch_child_modulepreload_link_handle_event(
                        scope,
                        owner,
                        link_handle,
                        successful,
                    );
                if !dispatched {
                    return Err(anyhow!(
                        "child modulepreload link target {link_handle:?} is no longer current"
                    ));
                }
                Ok(())
            })
    }
}
