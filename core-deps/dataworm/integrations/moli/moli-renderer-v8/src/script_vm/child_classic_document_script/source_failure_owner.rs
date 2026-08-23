use crate::{
    document_script_scheduler::{
        FrameDocumentClassicSourceFailureWork, ParserClassicDocumentScriptSourceFailureReport,
    },
    frame_owner_model::{
        FrameDocumentClassicScriptCompletionAction, FrameDocumentClassicScriptSourceFailureTarget,
        FrameDocumentClassicSourceFailureReportFollowup,
        FrameDocumentClassicSourceFailureReportSkipReason,
    },
};

use super::super::{
    ScriptVm,
    child_document_script_owner_hooks::{
        ChildDocumentScriptOwnerHooks, ChildDocumentScriptRealmSelection,
    },
};

pub(in crate::script_vm) struct ChildClassicSourceFailureOwner<'vm> {
    vm: &'vm mut ScriptVm,
}

impl<'vm> ChildClassicSourceFailureOwner<'vm> {
    pub(in crate::script_vm) fn new(vm: &'vm mut ScriptVm) -> Self {
        Self { vm }
    }

    pub(in crate::script_vm) fn report_source_failure(
        &mut self,
        failed: FrameDocumentClassicSourceFailureWork,
    ) -> anyhow::Result<
        ParserClassicDocumentScriptSourceFailureReport<
            FrameDocumentClassicScriptCompletionAction,
            FrameDocumentClassicSourceFailureReportFollowup,
        >,
    > {
        let target = *failed.target();
        let script_handle = failed.script_handle();
        tracing::warn!(
            child_handle = ?target.child_handle(),
            script_handle = ?failed.script_handle(),
            url = %failed.script_url(),
            error = failed.error(),
            "child external classic script load failed"
        );
        let mut followup = FrameDocumentClassicSourceFailureReportFollowup::default();
        followup.note_failure_logged();

        let expected_realm_id = target.realm_id();
        let current_realm_id = match ChildDocumentScriptOwnerHooks::new(self.vm)
            .select_current_realm(
                target.child_handle(),
                expected_realm_id,
                failed.script_handle(),
                "child_classic_source_failure",
            ) {
            ChildDocumentScriptRealmSelection::Current(realm_id) => realm_id,
            ChildDocumentScriptRealmSelection::RealmMaterializationFailed => {
                followup.note_skipped(
                    FrameDocumentClassicSourceFailureReportSkipReason::RealmMaterializationFailed,
                );
                ChildDocumentScriptOwnerHooks::new(self.vm)
                    .complete_child_deferred_classic_terminal_without_event(target, script_handle);
                return Ok(ParserClassicDocumentScriptSourceFailureReport::new(
                    None, followup,
                ));
            }
            ChildDocumentScriptRealmSelection::MissingCurrentRealm => {
                followup.note_skipped(
                    FrameDocumentClassicSourceFailureReportSkipReason::MissingCurrentRealm,
                );
                ChildDocumentScriptOwnerHooks::new(self.vm)
                    .complete_child_deferred_classic_terminal_without_event(target, script_handle);
                return Ok(ParserClassicDocumentScriptSourceFailureReport::new(
                    None, followup,
                ));
            }
            ChildDocumentScriptRealmSelection::StaleRealm { .. } => {
                followup
                    .note_skipped(FrameDocumentClassicSourceFailureReportSkipReason::StaleRealm);
                ChildDocumentScriptOwnerHooks::new(self.vm)
                    .complete_child_deferred_classic_terminal_without_event(target, script_handle);
                return Ok(ParserClassicDocumentScriptSourceFailureReport::new(
                    None, followup,
                ));
            }
        };

        let failed = failed.map_target(
            FrameDocumentClassicScriptSourceFailureTarget::new(
                target.child_handle(),
                target.task_owner(),
                Some(current_realm_id),
            )
            .with_scheduling(target.scheduling())
            .with_pending_script_key(target.pending_script_key())
            .with_load_delay_token(target.load_delay_token()),
        );

        let application = ChildDocumentScriptOwnerHooks::new(self.vm)
            .report_child_classic_script_source_failure(failed);
        if let Some(reason) = application.skip_reason() {
            followup.note_skipped(reason);
        }
        let completion = application.into_completion();
        if completion.is_some() {
            followup.note_completion_produced();
        } else {
            ChildDocumentScriptOwnerHooks::new(self.vm)
                .complete_child_deferred_classic_terminal_without_event(target, script_handle);
        }
        Ok(ParserClassicDocumentScriptSourceFailureReport::new(
            completion, followup,
        ))
    }
}
