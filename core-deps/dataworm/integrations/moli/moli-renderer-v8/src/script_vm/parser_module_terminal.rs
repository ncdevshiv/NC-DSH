use super::PreparedScriptBodyActivity;
use crate::host::ScriptEventTask;

/// The point at which a parser-owned module released parser ownership.
///
/// A pending top-level-await evaluation and a synchronously completed
/// evaluation both release the ordered parser lane in Moli, but they are
/// deliberately kept distinct so later terminal-event policy cannot infer one
/// from the other.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ParserModuleEvaluationSettlement {
    Completed,
    Suspended,
}

/// Where a parser-owned module's observable terminal is committed.
///
/// A parser-owned module terminal belongs to the concrete task that selected
/// that module action. Ordered defer/module work returns it to the parser
/// continuation; parse-time async module work returns it to the typed
/// main-Document module continuation. Both paths dispatch the terminal body
/// before their own task-end checkpoint. Runtime-owned module work keeps its
/// DynamicScriptOwner completion path.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ParserModuleTerminalDisposition {
    /// Keep terminal dispatch and its existing checkpoint inside module
    /// settlement. This is the unmigrated/runtime-owned path; it must not be
    /// combined with a selected-task completion at the caller.
    CompleteWithinModuleSettlement,
    /// Return the terminal body to the concrete selected task, which then
    /// owns the ordinary task-end checkpoint.
    ReturnToSelectedParserTask,
}

/// Observable terminal work produced while settling one parser-owned module.
///
/// This value is consumed by the selected parser action. It is not a queued
/// lifecycle task and must not outlive the exact parser settlement that
/// produced it.
#[derive(Debug)]
pub(crate) struct ParserOwnedModuleSuccessTerminal {
    evaluation: ParserModuleEvaluationSettlement,
    script_event: Option<ScriptEventTask>,
    prepared_activity: PreparedScriptBodyActivity,
}

impl ParserOwnedModuleSuccessTerminal {
    pub(crate) const fn new(
        evaluation: ParserModuleEvaluationSettlement,
        script_event: Option<ScriptEventTask>,
        prepared_activity: PreparedScriptBodyActivity,
    ) -> Self {
        Self {
            evaluation,
            script_event,
            prepared_activity,
        }
    }

    pub(crate) const fn evaluation(&self) -> ParserModuleEvaluationSettlement {
        self.evaluation
    }

    pub(crate) fn into_parts(self) -> (Option<ScriptEventTask>, PreparedScriptBodyActivity) {
        (self.script_event, self.prepared_activity)
    }
}

/// Result of finishing the execution-start side of a prepared module.
///
/// Runtime-owned modules retain their existing DynamicScriptOwner terminal
/// path. Parser-owned modules return their observable terminal to the concrete
/// selected task instead of publishing an anonymous lifecycle follow-up.
#[derive(Debug)]
pub(crate) enum PreparedModuleSuccessSettlement {
    ParserOwned(ParserOwnedModuleSuccessTerminal),
    ParserOwnedCompleted,
    RuntimeOwned,
    Stale,
}
