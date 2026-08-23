use super::context::FrameParserClassicScriptContext;
use super::owner::{
    FrameParserExternalLoadOwner, FrameParserRunnerTaskOwner, FrameParserScriptOwner,
    FrameParserSourceLoadClientOwner, FrameParserSourceLoadCompletionOwner,
    FrameParserSourceResultOwner,
};
use super::pending::FrameParserClassicScriptItem;
use crate::{
    document_runtime::DomHandle,
    document_script_scheduler::FrameDocumentClassicScriptSchedulerWork,
    frame_owner_model::{
        FrameDocumentClassicScriptBeginExecutionAction, FrameDocumentClassicScriptCompletionAction,
        FrameDocumentClassicScriptScheduling, FrameDocumentClassicScriptSourceLoadClient,
        FrameDocumentClassicScriptSourceLoadCompletionAction,
        FrameDocumentClassicScriptSourceLoadRequest, FrameDocumentOwner, FrameDocumentTaskOwner,
        FrameRealmId, FrameRequestId,
    },
    parser_script::{
        action::ParserPendingClassicScriptNotification, payload::ParserClassicScriptSourceResult,
        runner::ParserClassicScriptRunner,
    },
    types::ChildClassicScriptLoadCompletion,
};

// Minimal child-frame equivalent of Blink's HTMLParserScriptRunner for the
// parser-connected classic script subset.
#[derive(Debug, Clone)]
pub(crate) struct FrameParserClassicScriptRunner {
    owner: FrameDocumentOwner,
    parser_runner: ParserClassicScriptRunner<FrameParserClassicScriptContext>,
}

impl FrameParserClassicScriptRunner {
    pub(crate) fn empty(owner: FrameDocumentOwner) -> Self {
        Self {
            owner,
            parser_runner: ParserClassicScriptRunner::empty(),
        }
    }

    pub(crate) fn accepts_owner_document_handle(&self, owner_document_handle: DomHandle) -> bool {
        self.parser_runner.all_script_contexts_match(|context| {
            context.owner_document_handle() == owner_document_handle
        })
    }

    pub(crate) fn push_prepared_parser_script(
        &mut self,
        owner_document_handle: DomHandle,
        pending_script: FrameParserClassicScriptItem,
    ) -> bool {
        if !self.accepts_owner_document_handle(owner_document_handle) {
            return false;
        }
        self.parser_runner
            .push_parser_blocking_script(pending_script);
        true
    }

    #[cfg(test)]
    pub(crate) fn is_complete(&self) -> bool {
        self.parser_runner.is_complete()
    }

    fn parser_script_owner(
        &self,
        child_handle: DomHandle,
        task_owner: FrameDocumentTaskOwner,
        realm_id: Option<FrameRealmId>,
        scheduling: FrameDocumentClassicScriptScheduling,
        pending_script_key: Option<crate::document_script_scheduler::ParserPendingScriptKey>,
        load_delay_token: Option<crate::frame_owner_model::DocumentLoadDelayTokenId>,
        owner_current: bool,
    ) -> Option<FrameParserScriptOwner> {
        debug_assert_eq!(task_owner.document_owner(), self.owner);
        Some(FrameParserScriptOwner {
            child_handle,
            task_owner,
            realm_id,
            scheduling,
            pending_script_key,
            load_delay_token,
            owner_current,
        })
    }

    pub(crate) fn next_parser_blocking_task(
        &mut self,
        child_handle: DomHandle,
        task_owner: FrameDocumentTaskOwner,
        realm_id: Option<FrameRealmId>,
        owner_current: bool,
    ) -> Option<FrameDocumentClassicScriptSchedulerWork> {
        let mut owner = FrameParserRunnerTaskOwner {
            child_handle,
            task_owner,
            realm_id,
            scheduling: FrameDocumentClassicScriptScheduling::ParserBlocking,
            owner_current,
        };
        self.parser_runner
            .take_current_parser_blocking_next_action_with_owner(&mut owner)
    }

    pub(crate) fn next_deferred_task(
        &mut self,
        child_handle: DomHandle,
        task_owner: FrameDocumentTaskOwner,
        realm_id: Option<FrameRealmId>,
        owner_current: bool,
    ) -> Option<FrameDocumentClassicScriptSchedulerWork> {
        let mut owner = FrameParserRunnerTaskOwner {
            child_handle,
            task_owner,
            realm_id,
            scheduling: FrameDocumentClassicScriptScheduling::Deferred,
            owner_current,
        };
        self.parser_runner
            .take_current_deferred_next_action_with_owner(&mut owner)
    }

    pub(crate) fn current_deferred_script_key(
        &self,
    ) -> Option<crate::document_script_scheduler::ParserPendingScriptKey> {
        self.parser_runner
            .current_deferred_context()
            .map(FrameParserClassicScriptContext::pending_script_key)
    }

    pub(crate) fn discard_current_deferred_script_if_key(
        &mut self,
        key: crate::document_script_scheduler::ParserPendingScriptKey,
    ) -> bool {
        if self.current_deferred_script_key() != Some(key) {
            return false;
        }
        self.parser_runner
            .discard_current_deferred_script_if_handle(key.script_node_id())
    }

    pub(crate) fn current_parser_blocking_stylesheet_signatures(
        &self,
    ) -> Option<
        &std::collections::HashSet<crate::stylesheet_blocking::DocumentBlockingStylesheetSignature>,
    > {
        Some(
            self.parser_runner
                .current_parser_blocking_context()?
                .blocking_stylesheet_signatures(),
        )
    }

    pub(crate) fn has_current_parser_blocking_script(&self) -> bool {
        self.parser_runner
            .current_parser_blocking_context()
            .is_some()
    }

    pub(crate) fn current_deferred_stylesheet_signatures(
        &self,
    ) -> Option<
        &std::collections::HashSet<crate::stylesheet_blocking::DocumentBlockingStylesheetSignature>,
    > {
        Some(
            self.parser_runner
                .current_deferred_context()?
                .blocking_stylesheet_signatures(),
        )
    }

    pub(crate) fn begin_ready_execution(
        &mut self,
        child_handle: DomHandle,
        task_owner: FrameDocumentTaskOwner,
        realm_id: Option<FrameRealmId>,
        scheduling: FrameDocumentClassicScriptScheduling,
        pending_script_key: Option<crate::document_script_scheduler::ParserPendingScriptKey>,
        script_handle: DomHandle,
        owner_current: bool,
    ) -> Option<FrameDocumentClassicScriptBeginExecutionAction> {
        let load_delay_token = match scheduling {
            FrameDocumentClassicScriptScheduling::ParserBlocking => self
                .parser_runner
                .current_parser_blocking_context()
                .and_then(FrameParserClassicScriptContext::load_delay_token),
            FrameDocumentClassicScriptScheduling::Deferred => self
                .parser_runner
                .current_deferred_context()
                .and_then(FrameParserClassicScriptContext::load_delay_token),
        };
        let mut owner = self.parser_script_owner(
            child_handle,
            task_owner,
            realm_id,
            scheduling,
            pending_script_key,
            load_delay_token,
            owner_current,
        )?;
        match scheduling {
            FrameDocumentClassicScriptScheduling::ParserBlocking => self
                .parser_runner
                .take_current_parser_blocking_begin_execution_action_with_owner(
                    script_handle,
                    &mut owner,
                ),
            FrameDocumentClassicScriptScheduling::Deferred => self
                .parser_runner
                .take_current_deferred_begin_execution_action_with_owner(script_handle, &mut owner),
        }
    }

    pub(crate) fn dispose_ready_script(
        &mut self,
        child_handle: DomHandle,
        task_owner: FrameDocumentTaskOwner,
        realm_id: Option<FrameRealmId>,
        scheduling: FrameDocumentClassicScriptScheduling,
        pending_script_key: Option<crate::document_script_scheduler::ParserPendingScriptKey>,
        script_handle: DomHandle,
        owner_current: bool,
    ) -> Option<FrameDocumentClassicScriptCompletionAction> {
        let load_delay_token = match scheduling {
            FrameDocumentClassicScriptScheduling::ParserBlocking => self
                .parser_runner
                .current_parser_blocking_context()
                .and_then(FrameParserClassicScriptContext::load_delay_token),
            FrameDocumentClassicScriptScheduling::Deferred => self
                .parser_runner
                .current_deferred_context()
                .and_then(FrameParserClassicScriptContext::load_delay_token),
        };
        let mut owner = self.parser_script_owner(
            child_handle,
            task_owner,
            realm_id,
            scheduling,
            pending_script_key,
            load_delay_token,
            owner_current,
        )?;
        match scheduling {
            FrameDocumentClassicScriptScheduling::ParserBlocking => self
                .parser_runner
                .take_current_parser_blocking_disposed_ready_action_with_owner(
                    script_handle,
                    &mut owner,
                ),
            FrameDocumentClassicScriptScheduling::Deferred => self
                .parser_runner
                .take_current_deferred_disposed_ready_action_with_owner(script_handle, &mut owner),
        }
    }

    pub(crate) fn finish_executing(
        &mut self,
        child_handle: DomHandle,
        task_owner: FrameDocumentTaskOwner,
        realm_id: Option<FrameRealmId>,
        scheduling: FrameDocumentClassicScriptScheduling,
        pending_script_key: Option<crate::document_script_scheduler::ParserPendingScriptKey>,
        script_handle: DomHandle,
        owner_current: bool,
    ) -> Option<FrameDocumentClassicScriptCompletionAction> {
        let load_delay_token = match scheduling {
            FrameDocumentClassicScriptScheduling::ParserBlocking => self
                .parser_runner
                .current_parser_blocking_context()
                .and_then(FrameParserClassicScriptContext::load_delay_token),
            FrameDocumentClassicScriptScheduling::Deferred => self
                .parser_runner
                .current_deferred_context()
                .and_then(FrameParserClassicScriptContext::load_delay_token),
        };
        let mut owner = self.parser_script_owner(
            child_handle,
            task_owner,
            realm_id,
            scheduling,
            pending_script_key,
            load_delay_token,
            owner_current,
        )?;
        match scheduling {
            FrameDocumentClassicScriptScheduling::ParserBlocking => self
                .parser_runner
                .take_current_parser_blocking_finished_execution_action_with_owner(
                    script_handle,
                    &mut owner,
                ),
            FrameDocumentClassicScriptScheduling::Deferred => self
                .parser_runner
                .take_current_deferred_finished_execution_action_with_owner(
                    script_handle,
                    &mut owner,
                ),
        }
    }

    pub(crate) fn source_load_client(
        &self,
        child_handle: DomHandle,
    ) -> Option<FrameDocumentClassicScriptSourceLoadClient> {
        let mut owner = FrameParserSourceLoadClientOwner {
            child_handle,
            owner: self.owner,
        };
        self.parser_runner
            .current_parser_blocking_source_load_client_action_with_owner(&mut owner)
    }

    pub(crate) fn begin_external_load(
        &mut self,
        client: &FrameDocumentClassicScriptSourceLoadClient,
        load_id: u64,
        task_owner: FrameDocumentTaskOwner,
        owner_request_id: FrameRequestId,
    ) -> Option<FrameDocumentClassicScriptSourceLoadRequest> {
        let target = client.target();
        let mut owner = FrameParserExternalLoadOwner {
            child_handle: target.child_handle(),
            current_owner: self.owner,
            client_owner: target.owner(),
            client_metadata: client.metadata(),
            client_script_url: client.script_url().clone(),
            task_owner,
            owner_request_id,
        };
        self.parser_runner
            .take_current_parser_blocking_external_load_action_with_owner(load_id, &mut owner)
    }

    pub(crate) fn fail_external_pending_before_load(
        &mut self,
        client: &FrameDocumentClassicScriptSourceLoadClient,
        error: String,
    ) -> bool {
        if client.target().owner() != self.owner {
            return false;
        }
        self.parser_runner
            .fail_current_parser_blocking_external_pending_before_load(
                client.metadata(),
                client.script_url(),
                error,
            )
    }

    pub(crate) fn push_deferred_external_script_and_begin_load(
        &mut self,
        child_handle: DomHandle,
        pending_script: FrameParserClassicScriptItem,
        load_id: u64,
        task_owner: FrameDocumentTaskOwner,
        owner_request_id: FrameRequestId,
    ) -> Option<FrameDocumentClassicScriptSourceLoadRequest> {
        let client_metadata = pending_script.runner_metadata()?;
        let client_script_url = pending_script.runner_external_pending_script_url()?.clone();
        let mut owner = FrameParserExternalLoadOwner {
            child_handle,
            current_owner: self.owner,
            client_owner: self.owner,
            client_metadata,
            client_script_url,
            task_owner,
            owner_request_id,
        };
        self.parser_runner
            .push_deferred_external_script_with_load_action(pending_script, load_id, &mut owner)
    }

    pub(crate) fn external_load_owner(
        &self,
        completion: &ChildClassicScriptLoadCompletion,
    ) -> Option<FrameDocumentClassicScriptSourceLoadCompletionAction> {
        let mut owner = FrameParserSourceLoadCompletionOwner { completion };
        self.parser_runner
            .current_parser_blocking_source_load_completion_action_with_owner(&mut owner)
            .or_else(|| {
                self.parser_runner
                    .deferred_source_load_completion_action_with_owner(&mut owner)
            })
    }

    pub(crate) fn notify_external_source_result(
        &mut self,
        source_result: ParserClassicScriptSourceResult,
        owner_current: bool,
    ) -> Option<ParserPendingClassicScriptNotification> {
        let mut owner = FrameParserSourceResultOwner { owner_current };
        self.parser_runner
            .take_current_parser_blocking_source_result_action_with_owner(
                source_result.clone(),
                &mut owner,
            )
            .or_else(|| {
                self.parser_runner
                    .take_deferred_source_result_action_with_owner(source_result, &mut owner)
            })
    }
}
