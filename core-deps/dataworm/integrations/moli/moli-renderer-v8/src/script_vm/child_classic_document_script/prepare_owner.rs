use crate::{
    document_script_scheduler::{
        FrameDocumentClassicReadyWork, ParserClassicDocumentScriptExecutionStartReport,
    },
    frame_owner_model::{
        FrameClassicDocumentScriptExecutionStart, FrameDocumentClassicPrepareDropReason,
        FrameDocumentClassicPrepareFollowup, FrameDocumentClassicScriptReadyTarget,
    },
};

use super::super::{
    ScriptVm,
    child_document_script_owner_hooks::{
        ChildDocumentScriptOwnerHooks, ChildDocumentScriptRealmSelection,
    },
};

pub(in crate::script_vm) struct ChildClassicExecutionPrepareOwner<'vm> {
    vm: &'vm mut ScriptVm,
}

impl<'vm> ChildClassicExecutionPrepareOwner<'vm> {
    pub(in crate::script_vm) fn new(vm: &'vm mut ScriptVm) -> Self {
        Self { vm }
    }

    pub(in crate::script_vm) fn prepare_execution(
        &mut self,
        ready: FrameDocumentClassicReadyWork,
    ) -> ParserClassicDocumentScriptExecutionStartReport<
        crate::frame_owner_model::FrameClassicDocumentScriptExecutionAction,
        crate::frame_owner_model::FrameDocumentClassicScriptCompletionAction,
        FrameDocumentClassicPrepareFollowup,
    > {
        let mut followup = FrameDocumentClassicPrepareFollowup::default();
        let target = *ready.target();
        let script_handle = ready.script_handle();
        let expected_realm_id = target.realm_id();
        followup.note_realm_materialization_attempted();
        let current_realm_id = match ChildDocumentScriptOwnerHooks::new(self.vm)
            .select_current_realm(
                target.child_handle(),
                expected_realm_id,
                ready.script_handle(),
                "child_classic_prepare_execution",
            ) {
            ChildDocumentScriptRealmSelection::Current(realm_id) => {
                followup.note_realm_materialized();
                realm_id
            }
            ChildDocumentScriptRealmSelection::RealmMaterializationFailed => {
                followup.note_dropped(
                    FrameDocumentClassicPrepareDropReason::RealmMaterializationFailed,
                );
                ChildDocumentScriptOwnerHooks::new(self.vm)
                    .cancel_child_deferred_classic_ready_work(target, script_handle);
                return ParserClassicDocumentScriptExecutionStartReport::new(
                    FrameClassicDocumentScriptExecutionStart::Dropped,
                    followup,
                );
            }
            ChildDocumentScriptRealmSelection::MissingCurrentRealm => {
                followup.note_dropped(FrameDocumentClassicPrepareDropReason::MissingCurrentRealm);
                ChildDocumentScriptOwnerHooks::new(self.vm)
                    .cancel_child_deferred_classic_ready_work(target, script_handle);
                return ParserClassicDocumentScriptExecutionStartReport::new(
                    FrameClassicDocumentScriptExecutionStart::Dropped,
                    followup,
                );
            }
            ChildDocumentScriptRealmSelection::StaleRealm { .. } => {
                followup.note_dropped(FrameDocumentClassicPrepareDropReason::StaleRealm);
                ChildDocumentScriptOwnerHooks::new(self.vm)
                    .cancel_child_deferred_classic_ready_work(target, script_handle);
                return ParserClassicDocumentScriptExecutionStartReport::new(
                    FrameClassicDocumentScriptExecutionStart::Dropped,
                    followup,
                );
            }
        };
        let ready = ready.map_target(
            FrameDocumentClassicScriptReadyTarget::new(
                target.child_handle(),
                target.task_owner(),
                Some(current_realm_id),
                target.original_owner_document_handle(),
            )
            .with_scheduling(target.scheduling())
            .with_pending_script_key(target.pending_script_key())
            .with_load_delay_token(target.load_delay_token()),
        );
        let application = ChildDocumentScriptOwnerHooks::new(self.vm)
            .prepare_child_classic_script_execution(ready);
        if let Some(reason) = application.drop_reason() {
            followup.note_dropped(reason);
        }
        let start = application.into_start();
        match start {
            FrameClassicDocumentScriptExecutionStart::Execute(_) => {
                followup.note_execution_prepared();
            }
            FrameClassicDocumentScriptExecutionStart::Complete(_) => {
                followup.note_completion_produced();
            }
            FrameClassicDocumentScriptExecutionStart::Dropped => {
                ChildDocumentScriptOwnerHooks::new(self.vm)
                    .cancel_child_deferred_classic_ready_work(target, script_handle);
            }
        }
        ParserClassicDocumentScriptExecutionStartReport::new(start, followup)
    }
}
