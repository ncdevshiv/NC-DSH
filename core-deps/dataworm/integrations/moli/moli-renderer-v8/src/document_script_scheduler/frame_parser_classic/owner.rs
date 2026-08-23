use super::context::FrameParserClassicScriptContext;
use crate::{
    document_runtime::DomHandle,
    document_script_scheduler::{
        FrameDocumentClassicReadyWork, FrameDocumentClassicScriptSchedulerWork,
        FrameDocumentClassicSourceFailureWork,
    },
    frame_owner_model::{
        DocumentLoadDelayTokenId, FrameDocumentClassicScriptBeginExecutionAction,
        FrameDocumentClassicScriptCompletionAction, FrameDocumentClassicScriptCompletionTarget,
        FrameDocumentClassicScriptReadyTarget, FrameDocumentClassicScriptScheduling,
        FrameDocumentClassicScriptSourceFailureTarget, FrameDocumentClassicScriptSourceLoadClient,
        FrameDocumentClassicScriptSourceLoadCompletionAction,
        FrameDocumentClassicScriptSourceLoadOwner, FrameDocumentClassicScriptSourceLoadRequest,
        FrameDocumentOwner, FrameDocumentScriptElementEvent, FrameDocumentTaskOwner, FrameRealmId,
        FrameRequestId, frame_document_classic_script_begin_execution_action,
        frame_document_classic_script_source_load_client_action,
        frame_document_classic_script_source_load_completion_action,
        frame_document_classic_script_source_load_request_action,
    },
    parser_script::{
        action::{
            ParserPendingClassicScriptBeginExecutionAction,
            ParserPendingClassicScriptDisposedReadyAction,
            ParserPendingClassicScriptFinishedExecutionAction,
            ParserPendingClassicScriptNotification, ParserPendingClassicScriptReadyAction,
            ParserPendingClassicScriptReadyKind, ParserPendingClassicScriptSourceFailureAction,
            ParserPendingClassicScriptSourceLoadAction,
            ParserPendingClassicScriptSourceLoadCandidate,
            ParserPendingClassicScriptSourceLoadClientAction,
            ParserPendingClassicScriptSourceLoadCompletionAction,
            ParserPendingClassicScriptSourceResultAction,
        },
        context::ParserClassicScriptDocumentOwnerState,
        owner::{
            ParserScriptBeginExecutionOwner, ParserScriptBeginSourceLoadOwner,
            ParserScriptDisposeReadyOwner, ParserScriptFinishExecutionOwner, ParserScriptOwner,
            ParserScriptReadyOwner, ParserScriptSourceFailureOwner,
            ParserScriptSourceLoadClientOwner, ParserScriptSourceLoadCompletionOwner,
            ParserScriptSourceResultOwner,
        },
        payload::ParserClassicScriptMetadata,
    },
    types::ChildClassicScriptLoadCompletion,
};
use url::Url;

pub(super) struct FrameParserSourceResultOwner {
    pub(super) owner_current: bool,
}

pub(super) struct FrameParserExternalLoadOwner {
    pub(super) child_handle: DomHandle,
    pub(super) current_owner: FrameDocumentOwner,
    pub(super) client_owner: FrameDocumentOwner,
    pub(super) client_metadata: ParserClassicScriptMetadata,
    pub(super) client_script_url: Url,
    pub(super) task_owner: FrameDocumentTaskOwner,
    pub(super) owner_request_id: FrameRequestId,
}

pub(super) struct FrameParserSourceLoadClientOwner {
    pub(super) child_handle: DomHandle,
    pub(super) owner: FrameDocumentOwner,
}

pub(super) struct FrameParserSourceLoadCompletionOwner<'a> {
    pub(super) completion: &'a ChildClassicScriptLoadCompletion,
}

pub(super) struct FrameParserScriptOwner {
    pub(super) child_handle: DomHandle,
    pub(super) task_owner: FrameDocumentTaskOwner,
    pub(super) realm_id: Option<FrameRealmId>,
    pub(super) scheduling: FrameDocumentClassicScriptScheduling,
    pub(super) pending_script_key: Option<crate::document_script_scheduler::ParserPendingScriptKey>,
    pub(super) load_delay_token: Option<DocumentLoadDelayTokenId>,
    pub(super) owner_current: bool,
}

pub(super) struct FrameParserRunnerTaskOwner {
    pub(super) child_handle: DomHandle,
    pub(super) task_owner: FrameDocumentTaskOwner,
    pub(super) realm_id: Option<FrameRealmId>,
    pub(super) scheduling: FrameDocumentClassicScriptScheduling,
    pub(super) owner_current: bool,
}

impl ParserScriptOwner<FrameParserClassicScriptContext> for FrameParserSourceResultOwner {
    fn is_current_parser_script_owner(
        &mut self,
        _context: &FrameParserClassicScriptContext,
    ) -> bool {
        self.owner_current
    }
}

impl ParserScriptSourceResultOwner<FrameParserClassicScriptContext>
    for FrameParserSourceResultOwner
{
    type SourceResultAction = ParserPendingClassicScriptNotification;

    fn parser_script_source_result_action(
        &mut self,
        action: ParserPendingClassicScriptSourceResultAction<'_>,
    ) -> Option<Self::SourceResultAction> {
        let notification = action.notification();
        Some(notification)
    }
}

impl ParserScriptOwner<FrameParserClassicScriptContext> for FrameParserExternalLoadOwner {
    fn is_current_parser_script_owner(
        &mut self,
        context: &FrameParserClassicScriptContext,
    ) -> bool {
        context.parser_classic_document_task_owner() == self.task_owner
            && self.task_owner.document_owner() == self.client_owner
            && self.client_owner == self.current_owner
    }
}

impl ParserScriptBeginSourceLoadOwner<FrameParserClassicScriptContext>
    for FrameParserExternalLoadOwner
{
    type SourceLoadAction = FrameDocumentClassicScriptSourceLoadRequest;

    fn parser_script_source_load_candidate_matches(
        &mut self,
        candidate: ParserPendingClassicScriptSourceLoadCandidate<'_>,
    ) -> bool {
        candidate.metadata().script_handle() == self.client_metadata.script_handle()
            && candidate.script_url() == &self.client_script_url
    }

    fn parser_script_source_load_state(
        &mut self,
    ) -> Option<FrameDocumentClassicScriptSourceLoadOwner> {
        Some(FrameDocumentClassicScriptSourceLoadOwner {
            task_owner: self.task_owner,
            request_id: self.owner_request_id,
        })
    }

    fn parser_script_source_load_action(
        &mut self,
        action: ParserPendingClassicScriptSourceLoadAction,
    ) -> Option<Self::SourceLoadAction> {
        Some(frame_document_classic_script_source_load_request_action(
            self.child_handle,
            self.task_owner,
            self.owner_request_id,
            action,
        ))
    }
}

impl ParserScriptOwner<FrameParserClassicScriptContext> for FrameParserSourceLoadClientOwner {}

impl ParserScriptSourceLoadClientOwner<FrameParserClassicScriptContext>
    for FrameParserSourceLoadClientOwner
{
    type SourceLoadClientAction = FrameDocumentClassicScriptSourceLoadClient;

    fn parser_script_source_load_client_action(
        &mut self,
        client: ParserPendingClassicScriptSourceLoadClientAction<'_>,
    ) -> Option<Self::SourceLoadClientAction> {
        Some(frame_document_classic_script_source_load_client_action(
            self.child_handle,
            self.owner,
            client,
        ))
    }
}

impl ParserScriptOwner<FrameParserClassicScriptContext>
    for FrameParserSourceLoadCompletionOwner<'_>
{
    fn is_current_parser_script_owner(
        &mut self,
        context: &FrameParserClassicScriptContext,
    ) -> bool {
        context.parser_classic_document_task_owner() == self.completion.owner
    }
}

impl ParserScriptSourceLoadCompletionOwner<FrameParserClassicScriptContext>
    for FrameParserSourceLoadCompletionOwner<'_>
{
    type SourceLoadCompletionAction = FrameDocumentClassicScriptSourceLoadCompletionAction;

    fn parser_script_source_load_completion_action(
        &mut self,
        completion: ParserPendingClassicScriptSourceLoadCompletionAction<
            FrameDocumentClassicScriptSourceLoadOwner,
        >,
    ) -> Option<Self::SourceLoadCompletionAction> {
        let source_load_owner = completion.source_load_owner()?;
        let source_identity = completion.source_identity();
        if source_identity.metadata().script_handle() != self.completion.script_handle
            || source_identity.load_id() != Some(self.completion.load_id)
        {
            return None;
        }
        Some(frame_document_classic_script_source_load_completion_action(
            source_load_owner.task_owner,
            source_load_owner.request_id,
            completion,
        ))
    }
}

impl ParserScriptOwner<FrameParserClassicScriptContext> for FrameParserScriptOwner {
    fn is_current_parser_script_owner(
        &mut self,
        context: &FrameParserClassicScriptContext,
    ) -> bool {
        let captured_owner = context.parser_classic_document_task_owner();
        let is_current = self.owner_current && self.task_owner == captured_owner;
        if !is_current {
            tracing::debug!(
                child_handle = ?self.child_handle,
                ?captured_owner,
                task_owner = ?self.task_owner,
                owner_current = self.owner_current,
                "dropping stale child parser-classic execution action"
            );
        }
        is_current
    }
}

impl ParserScriptOwner<FrameParserClassicScriptContext> for FrameParserRunnerTaskOwner {
    fn is_current_parser_script_owner(
        &mut self,
        context: &FrameParserClassicScriptContext,
    ) -> bool {
        let captured_owner = context.parser_classic_document_task_owner();
        let is_current = self.owner_current && self.task_owner == captured_owner;
        if !is_current {
            tracing::debug!(
                child_handle = ?self.child_handle,
                ?captured_owner,
                task_owner = ?self.task_owner,
                owner_current = self.owner_current,
                "retaining child parser-classic PendingScript for a stale owner"
            );
        }
        is_current
    }
}

impl ParserScriptBeginExecutionOwner<FrameParserClassicScriptContext> for FrameParserScriptOwner {
    type BeginExecutionAction = FrameDocumentClassicScriptBeginExecutionAction;

    fn parser_script_begin_execution_action(
        &mut self,
        action: ParserPendingClassicScriptBeginExecutionAction,
    ) -> Option<Self::BeginExecutionAction> {
        Some(frame_document_classic_script_begin_execution_action(
            self.child_handle,
            self.task_owner,
            self.realm_id,
            self.scheduling,
            self.pending_script_key,
            self.load_delay_token,
            action,
        ))
    }
}

impl ParserScriptFinishExecutionOwner<FrameParserClassicScriptContext> for FrameParserScriptOwner {
    type FinishedExecutionAction = FrameDocumentClassicScriptCompletionAction;

    fn parser_script_finished_execution_action(
        &mut self,
        action: ParserPendingClassicScriptFinishedExecutionAction,
    ) -> Option<Self::FinishedExecutionAction> {
        let execution = action.execution();
        let script_element_event = (execution.ready_kind
            == ParserPendingClassicScriptReadyKind::External)
            .then_some(FrameDocumentScriptElementEvent::load(
                self.child_handle,
                self.task_owner.document_owner(),
                execution.metadata.script_handle(),
            ));
        let realm_id = self.realm_id?;
        Some(
            FrameDocumentClassicScriptCompletionAction::from_pending_finished_execution_action(
                FrameDocumentClassicScriptCompletionTarget::new(
                    self.child_handle,
                    self.task_owner,
                    realm_id,
                )
                .with_scheduling(self.scheduling)
                .with_pending_script_key(self.pending_script_key)
                .with_load_delay_token(self.load_delay_token),
                action,
                script_element_event,
            ),
        )
    }
}

impl ParserScriptReadyOwner<FrameParserClassicScriptContext> for FrameParserRunnerTaskOwner {
    type ReadyAction = FrameDocumentClassicScriptSchedulerWork;

    fn parser_script_ready_action(
        &mut self,
        context: &FrameParserClassicScriptContext,
        ready: ParserPendingClassicScriptReadyAction<'_>,
    ) -> Option<Self::ReadyAction> {
        let task_owner = context.parser_classic_document_task_owner();
        let pending_script_key = (self.scheduling
            == FrameDocumentClassicScriptScheduling::Deferred)
            .then(|| context.pending_script_key());
        Some(FrameDocumentClassicScriptSchedulerWork::Ready(
            FrameDocumentClassicReadyWork::from_pending_ready_action(
                FrameDocumentClassicScriptReadyTarget::new(
                    self.child_handle,
                    task_owner,
                    self.realm_id,
                    context.owner_document_handle(),
                )
                .with_scheduling(self.scheduling)
                .with_pending_script_key(pending_script_key)
                .with_load_delay_token(context.load_delay_token()),
                ready,
            ),
        ))
    }
}

impl ParserScriptDisposeReadyOwner<FrameParserClassicScriptContext> for FrameParserScriptOwner {
    type DisposedReadyAction = FrameDocumentClassicScriptCompletionAction;

    fn parser_script_disposed_ready_action(
        &mut self,
        action: ParserPendingClassicScriptDisposedReadyAction,
    ) -> Option<Self::DisposedReadyAction> {
        let realm_id = self.realm_id?;
        Some(
            FrameDocumentClassicScriptCompletionAction::from_pending_disposed_ready_action(
                FrameDocumentClassicScriptCompletionTarget::new(
                    self.child_handle,
                    self.task_owner,
                    realm_id,
                )
                .with_scheduling(self.scheduling)
                .with_pending_script_key(self.pending_script_key)
                .with_load_delay_token(self.load_delay_token),
                action,
            ),
        )
    }
}

impl ParserScriptSourceFailureOwner<FrameParserClassicScriptContext>
    for FrameParserRunnerTaskOwner
{
    type SourceFailureAction = FrameDocumentClassicScriptSchedulerWork;

    fn parser_script_source_failure_action(
        &mut self,
        context: &FrameParserClassicScriptContext,
        action: ParserPendingClassicScriptSourceFailureAction,
    ) -> Option<Self::SourceFailureAction> {
        let task_owner = context.parser_classic_document_task_owner();
        let pending_script_key = (self.scheduling
            == FrameDocumentClassicScriptScheduling::Deferred)
            .then(|| context.pending_script_key());
        let script_element_event = FrameDocumentScriptElementEvent::error(
            self.child_handle,
            task_owner.document_owner(),
            action.script_handle(),
        );
        Some(FrameDocumentClassicScriptSchedulerWork::SourceFailed(
            FrameDocumentClassicSourceFailureWork::from_pending_source_failure_action(
                FrameDocumentClassicScriptSourceFailureTarget::new(
                    self.child_handle,
                    task_owner,
                    self.realm_id,
                )
                .with_scheduling(self.scheduling)
                .with_pending_script_key(pending_script_key)
                .with_load_delay_token(context.load_delay_token()),
                action,
                Some(script_element_event),
            ),
        ))
    }
}
