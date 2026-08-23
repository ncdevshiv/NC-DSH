use crate::document_runtime::DomHandle;
use crate::parser_script::action::{
    ParserClassicScriptRunnerStep, ParserPendingClassicScriptExecution,
    ParserPendingClassicScriptNotification,
};
use crate::parser_script::payload::{
    ParserClassicScriptMetadata, ParserClassicScriptSourceFailure,
    ParserClassicScriptSourceIdentity, ParserClassicScriptSourceResult,
    ParserPreparedClassicScript,
};
use crate::parser_script::pending::ParserPendingClassicScriptEntry;
use crate::planning::PreparedScript;
use crate::types::SharedNavigationResponseResult;
use url::Url;

#[derive(Debug, Clone)]
pub(crate) struct ParserClassicScriptRunnerSlot {
    state: ParserClassicScriptRunnerSlotState,
}

impl ParserClassicScriptRunnerSlot {
    pub(crate) fn inline_ready(input: ParserPreparedClassicScript) -> Self {
        Self::pending(ParserPendingClassicScriptEntry::inline_ready(input))
    }

    pub(crate) fn external_pending(input: ParserPreparedClassicScript) -> Self {
        Self::pending(ParserPendingClassicScriptEntry::external_pending(input))
    }

    #[cfg(test)]
    pub(crate) fn from_pending_entry(pending_script: ParserPendingClassicScriptEntry) -> Self {
        Self::pending(pending_script)
    }

    fn pending(pending_script: ParserPendingClassicScriptEntry) -> Self {
        Self {
            state: ParserClassicScriptRunnerSlotState::Pending(pending_script),
        }
    }

    pub(crate) fn metadata(&self) -> Option<ParserClassicScriptMetadata> {
        match &self.state {
            ParserClassicScriptRunnerSlotState::Pending(pending_script) => {
                pending_script.metadata()
            }
            ParserClassicScriptRunnerSlotState::Executing(execution) => Some(execution.metadata),
            ParserClassicScriptRunnerSlotState::Finished
            | ParserClassicScriptRunnerSlotState::SourceFailed => None,
        }
    }

    pub(crate) fn script_handle(&self) -> Option<DomHandle> {
        Some(self.metadata()?.script_handle())
    }

    pub(crate) fn runner_step(&self) -> Option<ParserClassicScriptRunnerStep> {
        let ParserClassicScriptRunnerSlotState::Pending(pending_script) = &self.state else {
            return None;
        };
        pending_script.runner_step()
    }

    pub(crate) fn begin_ready_execution(
        &mut self,
        script_handle: DomHandle,
    ) -> Option<ParserPendingClassicScriptExecution> {
        if self.script_handle() != Some(script_handle) {
            return None;
        }
        let ParserClassicScriptRunnerSlotState::Pending(pending_script) = &self.state else {
            return None;
        };
        let execution = pending_script.ready_execution(script_handle)?;
        self.state = ParserClassicScriptRunnerSlotState::Executing(execution);
        Some(execution)
    }

    pub(crate) fn dispose_ready_without_execution(
        &mut self,
        script_handle: DomHandle,
    ) -> Option<ParserPendingClassicScriptExecution> {
        if self.script_handle() != Some(script_handle) {
            return None;
        }
        let ParserClassicScriptRunnerSlotState::Pending(pending_script) = &self.state else {
            return None;
        };
        let execution = pending_script.ready_execution(script_handle)?;
        self.state = ParserClassicScriptRunnerSlotState::Finished;
        Some(execution)
    }

    pub(crate) fn finish_execution(
        &mut self,
        script_handle: DomHandle,
    ) -> Option<ParserPendingClassicScriptExecution> {
        let ParserClassicScriptRunnerSlotState::Executing(execution) = &self.state else {
            return None;
        };
        if execution.metadata.script_handle() != script_handle {
            return None;
        }
        let execution = *execution;
        self.state = ParserClassicScriptRunnerSlotState::Finished;
        Some(execution)
    }

    pub(crate) fn finish_source_failure(&mut self) -> Option<ParserClassicScriptSourceFailure> {
        let ParserClassicScriptRunnerSlotState::Pending(pending_script) = &self.state else {
            return None;
        };
        let failure = pending_script.failed_source()?;
        self.state = ParserClassicScriptRunnerSlotState::SourceFailed;
        Some(failure)
    }

    pub(crate) fn external_pending_script_url(&self) -> Option<&Url> {
        let ParserClassicScriptRunnerSlotState::Pending(pending_script) = &self.state else {
            return None;
        };
        pending_script.external_pending_script_url()
    }

    pub(crate) fn begin_external_load(
        &mut self,
        load_id: u64,
    ) -> Option<(
        ParserClassicScriptSourceIdentity,
        ParserPreparedClassicScript,
    )> {
        let ParserClassicScriptRunnerSlotState::Pending(pending_script) = &mut self.state else {
            return None;
        };
        pending_script.begin_external_load(load_id)
    }

    pub(crate) fn begin_external_load_with_load_id(
        &mut self,
        load_id: Option<u64>,
    ) -> Option<(
        ParserClassicScriptSourceIdentity,
        ParserPreparedClassicScript,
    )> {
        let ParserClassicScriptRunnerSlotState::Pending(pending_script) = &mut self.state else {
            return None;
        };
        pending_script.begin_external_load_with_load_id(load_id)
    }

    pub(crate) fn fail_external_pending_before_load(&mut self, error: String) -> bool {
        let ParserClassicScriptRunnerSlotState::Pending(pending_script) = &mut self.state else {
            return false;
        };
        pending_script.fail_external_pending_before_load(error)
    }

    pub(crate) fn external_load_identity(
        &self,
    ) -> Option<(ParserClassicScriptSourceIdentity, Url)> {
        let ParserClassicScriptRunnerSlotState::Pending(pending_script) = &self.state else {
            return None;
        };
        pending_script.external_load_identity()
    }

    pub(crate) fn script(&self) -> Option<&PreparedScript> {
        let ParserClassicScriptRunnerSlotState::Pending(pending_script) = &self.state else {
            return None;
        };
        pending_script.script()
    }

    pub(crate) fn script_mut(&mut self) -> Option<&mut PreparedScript> {
        let ParserClassicScriptRunnerSlotState::Pending(pending_script) = &mut self.state else {
            return None;
        };
        pending_script.script_mut()
    }

    pub(crate) fn promote_external_pending_to_ready(&mut self) -> bool {
        let ParserClassicScriptRunnerSlotState::Pending(pending_script) = &mut self.state else {
            return false;
        };
        pending_script.promote_external_pending_to_ready()
    }

    pub(crate) fn notify_source_result_with_network_result(
        &mut self,
        source_result: ParserClassicScriptSourceResult,
    ) -> Option<(
        ParserPendingClassicScriptNotification,
        Option<SharedNavigationResponseResult>,
    )> {
        let ParserClassicScriptRunnerSlotState::Pending(pending_script) = &mut self.state else {
            return None;
        };
        pending_script.notify_source_result_with_network_result(source_result)
    }
}

#[derive(Debug, Clone)]
enum ParserClassicScriptRunnerSlotState {
    Pending(ParserPendingClassicScriptEntry),
    Executing(ParserPendingClassicScriptExecution),
    Finished,
    SourceFailed,
}
