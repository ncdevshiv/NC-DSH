use crate::document_script_scheduler::DocumentScriptExecutionOutcome;

/// Script-visible activity produced by one exact child document-script task.
///
/// Keep this fact separate from `DocumentScriptExecutionOutcome`: releasing a
/// parser order slot can make domain progress without entering JavaScript,
/// while a top-level script can enter V8 even if its surrounding scheduler
/// state reports no additional progress. Only the former controls output
/// capture; this value controls the selected task's completion boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ChildDocumentScriptActivity {
    NoScriptOrEvent,
    ScriptOrEvent,
}

/// Execution-produced result for either subset of `DocumentScriptReady`.
///
/// It is created after the exact payload has been authorized and consumed. It
/// must never be stored in the queued Page task or used for source selection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ChildDocumentScriptRunOutcome {
    execution: DocumentScriptExecutionOutcome,
    activity: ChildDocumentScriptActivity,
}

impl ChildDocumentScriptRunOutcome {
    pub(crate) const fn new(
        execution: DocumentScriptExecutionOutcome,
        activity: ChildDocumentScriptActivity,
    ) -> Self {
        Self {
            execution,
            activity,
        }
    }

    pub(crate) fn made_progress(self) -> bool {
        self.execution.made_progress()
    }

    pub(crate) const fn activity(self) -> ChildDocumentScriptActivity {
        self.activity
    }
}

/// Strong result of consuming one heterogeneous child `DocumentScriptReady`
/// payload.
///
/// The input lane remains heterogeneous so classic and module scripts preserve
/// document order. Once an exact body has run, both families intentionally
/// collapse into the same task-completion fact: module-specific algorithmic
/// checkpoints have already happened and do not create another HTML task.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ChildDocumentScriptReadyRunOutcome {
    Applied(ChildDocumentScriptRunOutcome),
    #[cfg(test)]
    DiscardedStale,
}

impl ChildDocumentScriptReadyRunOutcome {
    #[cfg(test)]
    pub(crate) fn made_progress(self) -> bool {
        match self {
            Self::Applied(outcome) => outcome.made_progress(),
            Self::DiscardedStale => false,
        }
    }
}
