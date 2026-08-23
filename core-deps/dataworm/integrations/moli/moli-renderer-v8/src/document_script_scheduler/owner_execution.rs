use crate::types::ScriptRun;

use super::{DocumentScriptExecutionLane, DocumentScriptSourceFailureLane};

/// The concrete body selected from main page-owned document-script work.
///
/// Script execution and source-failure settlement share one ordered carrier,
/// but they do not prove the same callback or module activity. Keeping the
/// body kind after execution lets P5 migrate their task-end boundaries without
/// recovering domain meaning from a queued task name.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PageOwnedDocumentScriptBodyKind {
    Script(DocumentScriptExecutionLane),
    SourceFailure(DocumentScriptSourceFailureLane),
}

/// Callback-capable activity produced while executing one selected body.
///
/// A module graph start, import-map registration, disabled script, or
/// source-failure bookkeeping can consume a current task without entering
/// script or event code. Those tasks still owe a checkpoint, but they must not
/// acquire callback-only child-record synchronization. Script evaluation and
/// an attempted synchronous script-element/window error dispatch use the
/// callback checkpoint path. This does not claim that an event target had a
/// listener or that every listener completed successfully, and it does not
/// authorize synchronous execution of another runtime task.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PageOwnedDocumentScriptBodyActivity {
    NoPageCodeOrEventDispatch,
    PageCodeOrEventDispatch,
}

/// Script report plus the activity observed by the concrete body executor.
///
/// This value is produced after execution. It is deliberately not a queued
/// policy: the scheduler cannot know whether a source failure attempted an
/// immediate event dispatch or only published a later lifecycle task until
/// the body has actually run.
#[must_use = "document-script body activity determines its task completion"]
#[derive(Debug)]
pub(crate) struct PageOwnedDocumentScriptBodyExecution {
    run: ScriptRun,
    activity: PageOwnedDocumentScriptBodyActivity,
}

impl PageOwnedDocumentScriptBodyExecution {
    pub(crate) fn without_page_code_or_event_dispatch(run: ScriptRun) -> Self {
        Self {
            run,
            activity: PageOwnedDocumentScriptBodyActivity::NoPageCodeOrEventDispatch,
        }
    }

    pub(crate) fn with_page_code_or_event_dispatch(run: ScriptRun) -> Self {
        Self {
            run,
            activity: PageOwnedDocumentScriptBodyActivity::PageCodeOrEventDispatch,
        }
    }

    fn into_parts(self) -> (ScriptRun, PageOwnedDocumentScriptBodyActivity) {
        (self.run, self.activity)
    }
}

/// Exact owners observed before and after one entered document-script body.
///
/// JavaScript or an error listener can synchronously replace the Document. A
/// changed owner is therefore not a stale admission: the body already ran
/// under `owner_before`, and its completion must preserve that fact. The main
/// page-owned adapter currently always enters a prepared body; an impossible
/// pre-body drop is rejected by the runner rather than represented as a
/// misleading production state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PageOwnedDocumentScriptOwnerTransition<DocumentOwnerToken> {
    owner_before: DocumentOwnerToken,
    owner_after_body: Option<DocumentOwnerToken>,
}

impl<DocumentOwnerToken> PageOwnedDocumentScriptOwnerTransition<DocumentOwnerToken> {
    const fn entered(
        owner_before: DocumentOwnerToken,
        owner_after_body: Option<DocumentOwnerToken>,
    ) -> Self {
        Self {
            owner_before,
            owner_after_body,
        }
    }

    pub(crate) const fn owner_before(&self) -> &DocumentOwnerToken {
        &self.owner_before
    }

    pub(crate) const fn owner_after_body(&self) -> Option<&DocumentOwnerToken> {
        self.owner_after_body.as_ref()
    }
}

/// Post-execution fact that the active carrier must consume.
///
/// This value deliberately does not claim that an HTML task-end checkpoint
/// has run. The body runner retains only the legacy pre-script compatibility
/// checkpoint and the script algorithm's internal boundaries; every carrier
/// must hand this fact to the shared task-completion coordinator. Keeping it
/// explicit prevents task-end authority from being erased into a generic
/// run/no-run report.
#[must_use = "document-script completion must reach its carrier boundary"]
#[derive(Debug)]
pub(crate) struct PageOwnedDocumentScriptCompletion<DocumentOwnerToken> {
    body: PageOwnedDocumentScriptBodyKind,
    activity: PageOwnedDocumentScriptBodyActivity,
    owner_transition: PageOwnedDocumentScriptOwnerTransition<DocumentOwnerToken>,
}

impl<DocumentOwnerToken> PageOwnedDocumentScriptCompletion<DocumentOwnerToken> {
    fn entered(
        body: PageOwnedDocumentScriptBodyKind,
        activity: PageOwnedDocumentScriptBodyActivity,
        owner_before: DocumentOwnerToken,
        owner_after_body: Option<DocumentOwnerToken>,
    ) -> Self {
        Self {
            body,
            activity,
            owner_transition: PageOwnedDocumentScriptOwnerTransition::entered(
                owner_before,
                owner_after_body,
            ),
        }
    }

    pub(crate) const fn body(&self) -> PageOwnedDocumentScriptBodyKind {
        self.body
    }

    pub(crate) const fn activity(&self) -> PageOwnedDocumentScriptBodyActivity {
        self.activity
    }

    pub(crate) const fn owner_transition(
        &self,
    ) -> PageOwnedDocumentScriptOwnerTransition<DocumentOwnerToken>
    where
        DocumentOwnerToken: Copy,
    {
        self.owner_transition
    }
}

/// Script report plus the non-erasable completion fact from one selected
/// page-owned document-script body.
#[must_use = "document-script execution must be recorded and completed"]
#[derive(Debug)]
pub(crate) struct PageOwnedDocumentScriptExecution<DocumentOwnerToken> {
    run: ScriptRun,
    completion: PageOwnedDocumentScriptCompletion<DocumentOwnerToken>,
}

impl<DocumentOwnerToken> PageOwnedDocumentScriptExecution<DocumentOwnerToken> {
    pub(super) fn entered(
        body: PageOwnedDocumentScriptBodyKind,
        owner_before: DocumentOwnerToken,
        owner_after_body: Option<DocumentOwnerToken>,
        body_execution: PageOwnedDocumentScriptBodyExecution,
    ) -> Self {
        let (run, activity) = body_execution.into_parts();
        Self {
            run,
            completion: PageOwnedDocumentScriptCompletion::entered(
                body,
                activity,
                owner_before,
                owner_after_body,
            ),
        }
    }

    pub(crate) fn into_parts(
        self,
    ) -> (
        ScriptRun,
        PageOwnedDocumentScriptCompletion<DocumentOwnerToken>,
    ) {
        (self.run, self.completion)
    }
}
