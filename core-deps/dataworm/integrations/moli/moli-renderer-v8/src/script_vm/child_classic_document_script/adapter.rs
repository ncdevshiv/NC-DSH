use crate::{
    document_script_scheduler::{
        DocumentScriptExecutionOutcome, FrameDocumentClassicReadyWork,
        FrameDocumentClassicSourceFailureWork, ParserClassicDocumentScriptCompletionPlan,
        ParserClassicDocumentScriptContinuation, ParserClassicDocumentScriptExecutionHooks,
        ParserClassicDocumentScriptExecutionResult,
        ParserClassicDocumentScriptExecutionStartReport,
        ParserClassicDocumentScriptSourceFailureReport,
    },
    frame_owner_model::{
        FrameClassicDocumentScriptExecutionAction, FrameDocumentClassicCompletionFinishAction,
        FrameDocumentClassicCompletionFollowup, FrameDocumentClassicCompletionScriptEventFollowup,
        FrameDocumentClassicExecutionFollowup, FrameDocumentClassicPrepareFollowup,
        FrameDocumentClassicScriptCompletionAction, FrameDocumentClassicScriptCompletionTarget,
        FrameDocumentClassicSourceFailureReportFollowup,
    },
};
use std::future::{Ready, ready};

use super::{
    super::{ChildDocumentScriptActivity, ChildDocumentScriptRunOutcome, ScriptVm},
    ChildClassicCompletionOwner, ChildClassicExecutionActionOwner,
    ChildClassicExecutionPrepareOwner, ChildClassicSourceFailureOwner,
};

pub(in crate::script_vm) struct ScriptVmChildClassicExecutionHooks<'vm> {
    vm: &'vm mut ScriptVm,
}

impl<'vm> ScriptVmChildClassicExecutionHooks<'vm> {
    pub(in crate::script_vm) fn new(vm: &'vm mut ScriptVm) -> Self {
        Self { vm }
    }
}

fn outcome_from_completion_followup(
    followup: FrameDocumentClassicCompletionFollowup,
) -> ChildDocumentScriptRunOutcome {
    let activity = if followup.script_event_was_dispatched() {
        ChildDocumentScriptActivity::ScriptOrEvent
    } else {
        ChildDocumentScriptActivity::NoScriptOrEvent
    };
    let execution = if followup.made_progress() {
        DocumentScriptExecutionOutcome::Progressed
    } else {
        DocumentScriptExecutionOutcome::NoProgress
    };
    ChildDocumentScriptRunOutcome::new(execution, activity)
}

fn outcome_from_prepare_followup(
    followup: FrameDocumentClassicPrepareFollowup,
) -> ChildDocumentScriptRunOutcome {
    let execution = if followup.made_progress() {
        DocumentScriptExecutionOutcome::Progressed
    } else {
        DocumentScriptExecutionOutcome::NoProgress
    };
    ChildDocumentScriptRunOutcome::new(execution, ChildDocumentScriptActivity::NoScriptOrEvent)
}

fn outcome_from_execution_followup(
    followup: FrameDocumentClassicExecutionFollowup,
) -> ChildDocumentScriptRunOutcome {
    let activity = if followup.script_job_was_attempted() {
        ChildDocumentScriptActivity::ScriptOrEvent
    } else {
        ChildDocumentScriptActivity::NoScriptOrEvent
    };
    let execution = if followup.made_progress() {
        DocumentScriptExecutionOutcome::Progressed
    } else {
        DocumentScriptExecutionOutcome::NoProgress
    };
    ChildDocumentScriptRunOutcome::new(execution, activity)
}

fn outcome_from_source_failure_report_followup(
    followup: FrameDocumentClassicSourceFailureReportFollowup,
) -> ChildDocumentScriptRunOutcome {
    let execution = if followup.made_progress() {
        DocumentScriptExecutionOutcome::Progressed
    } else {
        DocumentScriptExecutionOutcome::NoProgress
    };
    ChildDocumentScriptRunOutcome::new(execution, ChildDocumentScriptActivity::NoScriptOrEvent)
}

impl ParserClassicDocumentScriptExecutionHooks for ScriptVmChildClassicExecutionHooks<'_> {
    type Ready = FrameDocumentClassicReadyWork;
    type SourceFailure = FrameDocumentClassicSourceFailureWork;
    type ExecutionAction = FrameClassicDocumentScriptExecutionAction;
    type Completion = FrameDocumentClassicScriptCompletionAction;
    type CompletionAction = FrameDocumentClassicCompletionFinishAction;
    type CompletionContinuationAction = FrameDocumentClassicScriptCompletionTarget;
    type PrepareFollowup = FrameDocumentClassicPrepareFollowup;
    type CompletionEffectsFollowup = FrameDocumentClassicCompletionScriptEventFollowup;
    type CompletionFollowup = FrameDocumentClassicCompletionFollowup;
    type SourceFailureReportFollowup = FrameDocumentClassicSourceFailureReportFollowup;
    type PostExecution = FrameDocumentClassicExecutionFollowup;
    type Output = ChildDocumentScriptRunOutcome;
    type CompletionEffectsFuture<'owner>
        = Ready<anyhow::Result<FrameDocumentClassicCompletionScriptEventFollowup>>
    where
        Self: 'owner;
    type CompletionFuture<'owner>
        = Ready<anyhow::Result<FrameDocumentClassicCompletionFollowup>>
    where
        Self: 'owner;
    type ExecuteFuture<'owner>
        = Ready<
        anyhow::Result<
            ParserClassicDocumentScriptExecutionResult<Self::Completion, Self::PostExecution>,
        >,
    >
    where
        Self: 'owner;

    fn prepare_execution(
        &mut self,
        ready: FrameDocumentClassicReadyWork,
    ) -> ParserClassicDocumentScriptExecutionStartReport<
        FrameClassicDocumentScriptExecutionAction,
        FrameDocumentClassicScriptCompletionAction,
        FrameDocumentClassicPrepareFollowup,
    > {
        ChildClassicExecutionPrepareOwner::new(self.vm).prepare_execution(ready)
    }

    fn execute_action(
        &mut self,
        action: FrameClassicDocumentScriptExecutionAction,
    ) -> Self::ExecuteFuture<'_> {
        ready(Ok(
            ChildClassicExecutionActionOwner::new(self.vm).execute_action(action)
        ))
    }

    fn report_source_failure(
        &mut self,
        failed: FrameDocumentClassicSourceFailureWork,
    ) -> anyhow::Result<
        ParserClassicDocumentScriptSourceFailureReport<
            FrameDocumentClassicScriptCompletionAction,
            FrameDocumentClassicSourceFailureReportFollowup,
        >,
    > {
        ChildClassicSourceFailureOwner::new(self.vm).report_source_failure(failed)
    }

    fn prepare_completion_plan(
        &mut self,
        completion: FrameDocumentClassicScriptCompletionAction,
    ) -> anyhow::Result<
        ParserClassicDocumentScriptCompletionPlan<
            FrameDocumentClassicCompletionFinishAction,
            FrameDocumentClassicScriptCompletionTarget,
        >,
    > {
        Ok(ChildClassicCompletionOwner::new(self.vm).prepare_completion_plan(completion))
    }

    fn apply_completion_action(
        &mut self,
        action: FrameDocumentClassicCompletionFinishAction,
    ) -> Self::CompletionEffectsFuture<'_> {
        ready(ChildClassicCompletionOwner::new(self.vm).apply_completion_action(action))
    }

    fn apply_completion_continuation(
        &mut self,
        continuation: ParserClassicDocumentScriptContinuation<Self::CompletionContinuationAction>,
        effects: Self::CompletionEffectsFollowup,
    ) -> Self::CompletionFuture<'_> {
        ready(
            ChildClassicCompletionOwner::new(self.vm)
                .apply_completion_continuation(continuation, effects),
        )
    }

    fn outcome_after_executed_completion(
        &mut self,
        execution_followup: FrameDocumentClassicExecutionFollowup,
        completion_followup: FrameDocumentClassicCompletionFollowup,
    ) -> anyhow::Result<ChildDocumentScriptRunOutcome> {
        let execution_progressed = execution_followup.made_progress();
        let script_was_attempted = execution_followup.script_job_was_attempted();
        let completion = outcome_from_completion_followup(completion_followup);
        Ok(ChildDocumentScriptRunOutcome::new(
            if execution_progressed || completion.made_progress() {
                DocumentScriptExecutionOutcome::Progressed
            } else {
                DocumentScriptExecutionOutcome::NoProgress
            },
            if script_was_attempted
                || completion.activity() == ChildDocumentScriptActivity::ScriptOrEvent
            {
                ChildDocumentScriptActivity::ScriptOrEvent
            } else {
                ChildDocumentScriptActivity::NoScriptOrEvent
            },
        ))
    }

    fn outcome_after_completion(
        &mut self,
        completion_followup: FrameDocumentClassicCompletionFollowup,
    ) -> anyhow::Result<ChildDocumentScriptRunOutcome> {
        Ok(outcome_from_completion_followup(completion_followup))
    }

    fn outcome_for_dropped_ready(
        &mut self,
        prepare_followup: FrameDocumentClassicPrepareFollowup,
    ) -> anyhow::Result<ChildDocumentScriptRunOutcome> {
        Ok(outcome_from_prepare_followup(prepare_followup))
    }

    fn outcome_for_missing_execution_completion(
        &mut self,
        execution_followup: FrameDocumentClassicExecutionFollowup,
    ) -> anyhow::Result<ChildDocumentScriptRunOutcome> {
        Ok(outcome_from_execution_followup(execution_followup))
    }

    fn outcome_for_source_failure_without_completion(
        &mut self,
        report_followup: FrameDocumentClassicSourceFailureReportFollowup,
    ) -> anyhow::Result<ChildDocumentScriptRunOutcome> {
        Ok(outcome_from_source_failure_report_followup(report_followup))
    }
}
