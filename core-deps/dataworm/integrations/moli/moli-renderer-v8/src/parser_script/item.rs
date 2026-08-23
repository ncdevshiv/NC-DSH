use crate::document_runtime::DomHandle;
use crate::parser_script::action::{
    ParserClassicScriptRunnerStep, ParserPendingClassicScriptExecution,
    ParserPendingClassicScriptNotification, ParserPendingClassicScriptSourceLoadAction,
    ParserPendingClassicScriptSourceLoadRequest,
};
use crate::parser_script::context::{
    ParserClassicScriptExecutionGateState, ParserClassicScriptSourceLoadStartState,
};
use crate::parser_script::owner::ParserScriptBeginSourceLoadOwner;
use crate::parser_script::payload::{
    ParserClassicScriptMetadata, ParserClassicScriptSourceFailure,
    ParserClassicScriptSourceIdentity, ParserClassicScriptSourceResult,
    ParserPreparedClassicScript,
};
use crate::parser_script::slot::ParserClassicScriptRunnerSlot;
use crate::planning::PreparedScript;
use crate::types::SharedNavigationResponseResult;
use url::Url;

#[derive(Debug, Clone)]
pub(crate) struct ParserClassicScriptRunnerItem<C> {
    slot: ParserClassicScriptRunnerSlot,
    context: C,
}

impl<C> ParserClassicScriptRunnerItem<C> {
    pub(crate) fn inline_ready(input: ParserPreparedClassicScript, context: C) -> Self {
        Self::from_slot(ParserClassicScriptRunnerSlot::inline_ready(input), context)
    }

    pub(crate) fn external_pending(input: ParserPreparedClassicScript, context: C) -> Self {
        Self::from_slot(
            ParserClassicScriptRunnerSlot::external_pending(input),
            context,
        )
    }

    #[cfg(test)]
    pub(crate) fn from_slot_for_test(slot: ParserClassicScriptRunnerSlot, context: C) -> Self {
        Self::from_slot(slot, context)
    }

    fn from_slot(slot: ParserClassicScriptRunnerSlot, context: C) -> Self {
        Self { slot, context }
    }

    pub(crate) fn context(&self) -> &C {
        &self.context
    }

    pub(crate) fn context_mut(&mut self) -> &mut C {
        &mut self.context
    }

    pub(crate) fn runner_metadata(&self) -> Option<ParserClassicScriptMetadata> {
        self.slot.metadata()
    }

    pub(crate) fn runner_step(&self) -> Option<ParserClassicScriptRunnerStep> {
        self.slot.runner_step()
    }

    pub(crate) fn begin_runner_ready_execution(
        &mut self,
        script_handle: DomHandle,
    ) -> Option<ParserPendingClassicScriptExecution> {
        self.slot.begin_ready_execution(script_handle)
    }

    pub(crate) fn dispose_runner_ready_without_execution(
        &mut self,
        script_handle: DomHandle,
    ) -> Option<ParserPendingClassicScriptExecution> {
        self.slot.dispose_ready_without_execution(script_handle)
    }

    pub(crate) fn finish_runner_execution(
        &mut self,
        script_handle: DomHandle,
    ) -> Option<ParserPendingClassicScriptExecution> {
        self.slot.finish_execution(script_handle)
    }

    pub(crate) fn finish_runner_source_failure(
        &mut self,
    ) -> Option<ParserClassicScriptSourceFailure> {
        self.slot.finish_source_failure()
    }

    pub(crate) fn runner_external_pending_script_url(&self) -> Option<&Url> {
        self.slot.external_pending_script_url()
    }

    pub(crate) fn begin_runner_external_load(
        &mut self,
        load_id: u64,
    ) -> Option<(
        ParserClassicScriptSourceIdentity,
        ParserPreparedClassicScript,
    )> {
        self.slot.begin_external_load(load_id)
    }

    pub(crate) fn begin_runner_external_load_with_load_id(
        &mut self,
        load_id: Option<u64>,
    ) -> Option<(
        ParserClassicScriptSourceIdentity,
        ParserPreparedClassicScript,
    )> {
        self.slot.begin_external_load_with_load_id(load_id)
    }

    pub(crate) fn fail_runner_external_pending_before_load(&mut self, error: String) -> bool {
        self.slot.fail_external_pending_before_load(error)
    }

    pub(crate) fn begin_runner_external_load_with_load_id_and_owner<Owner>(
        &mut self,
        load_id: Option<u64>,
        owner: &mut Owner,
    ) -> Option<Owner::SourceLoadAction>
    where
        C: ParserClassicScriptExecutionGateState + ParserClassicScriptSourceLoadStartState,
        Owner: ParserScriptBeginSourceLoadOwner<C>,
    {
        if !owner.is_current_parser_script_owner(self.context()) {
            return None;
        }
        let (source_identity, input) = self.begin_runner_external_load_with_load_id(load_id)?;
        let request = ParserPendingClassicScriptSourceLoadRequest::new(source_identity, input);
        let action = ParserPendingClassicScriptSourceLoadAction::new(request);
        let owner_action = owner.parser_script_source_load_action(action)?;
        if let Some(state) = owner.parser_script_source_load_state() {
            self.context_mut()
                .install_parser_classic_source_load_state(state);
        }
        Some(owner_action)
    }

    pub(crate) fn runner_external_load_identity(
        &self,
    ) -> Option<(ParserClassicScriptSourceIdentity, Url)> {
        self.slot.external_load_identity()
    }

    pub(crate) fn runner_external_load_matches_source_result(
        &self,
        source_result: &ParserClassicScriptSourceResult,
    ) -> bool {
        self.runner_external_load_identity()
            .is_some_and(|(identity, _)| identity.matches_source_result(source_result))
    }

    pub(crate) fn runner_script(&self) -> Option<&PreparedScript> {
        self.slot.script()
    }

    pub(crate) fn runner_script_mut(&mut self) -> Option<&mut PreparedScript> {
        self.slot.script_mut()
    }

    pub(crate) fn promote_runner_external_pending_to_ready(&mut self) -> bool {
        self.slot.promote_external_pending_to_ready()
    }

    pub(crate) fn notify_runner_source_result_with_network_result(
        &mut self,
        source_result: ParserClassicScriptSourceResult,
    ) -> Option<(
        ParserPendingClassicScriptNotification,
        Option<SharedNavigationResponseResult>,
    )> {
        self.slot
            .notify_source_result_with_network_result(source_result)
    }
}
