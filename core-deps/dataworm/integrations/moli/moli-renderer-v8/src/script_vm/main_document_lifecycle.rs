//! Execution-time facts for main-document lifecycle work.
//!
//! These values are produced only after an exact lifecycle body is selected.
//! They are neither queue entries nor a second DCL/load authority. Keeping
//! target application, callback progress, settlement, and concrete follow-up
//! orthogonal prevents later checkpoint migration from guessing behavior from
//! a task name or a single boolean.

#[cfg(test)]
use super::NonScriptPageTaskExecutionOutcome;
use crate::frame_owner_model::{FrameDocumentTaskOwner, MainDocumentInteractiveLifecycleAction};
use crate::page_task_queue::{PageOwnedInternalLoadingTask, PostParseLifecycleWork};
use std::time::Instant;

/// The three main-document lifecycle bodies migrated together by P5-A.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MainDocumentLifecycleBody {
    Interactive(MainDocumentInteractiveLifecycleAction),
    DomContentLoaded { owner: FrameDocumentTaskOwner },
    WindowLoad { owner: FrameDocumentTaskOwner },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MainDocumentLifecycleBodyKind {
    Interactive,
    DomContentLoaded,
    WindowLoad,
}

impl MainDocumentLifecycleBody {
    pub(crate) fn from_post_parse_work(work: &PostParseLifecycleWork) -> Option<Self> {
        match work {
            PostParseLifecycleWork::ApplyMainDocumentInteractive(action) => {
                Some(Self::Interactive(*action))
            }
            PostParseLifecycleWork::DispatchDomContentLoaded { owner } => {
                Some(Self::DomContentLoaded { owner: *owner })
            }
            PostParseLifecycleWork::DispatchWindowLoad { owner } => {
                Some(Self::WindowLoad { owner: *owner })
            }
            _ => None,
        }
    }

    pub(crate) const fn kind(self) -> MainDocumentLifecycleBodyKind {
        match self {
            Self::Interactive(_) => MainDocumentLifecycleBodyKind::Interactive,
            Self::DomContentLoaded { .. } => MainDocumentLifecycleBodyKind::DomContentLoaded,
            Self::WindowLoad { .. } => MainDocumentLifecycleBodyKind::WindowLoad,
        }
    }

    pub(crate) fn owner(self) -> FrameDocumentTaskOwner {
        match self {
            Self::Interactive(action) => action.owner(),
            Self::DomContentLoaded { owner } | Self::WindowLoad { owner } => owner,
        }
    }
}

/// Why a selected lifecycle body did not apply its exact transition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MainDocumentLifecycleTargetRejection {
    /// PageVm observed an already-pending cross-Document navigation before it
    /// entered the DCL/load body.
    PendingCrossDocumentNavigation,
    /// The ScriptVm transition authority rejected a stale, blocked, or
    /// already-consumed exact owner action.
    TransitionRejected,
}

impl MainDocumentLifecycleTargetRejection {
    #[cfg(test)]
    pub(crate) const fn is_pending_cross_document_navigation(self) -> bool {
        matches!(self, Self::PendingCrossDocumentNavigation)
    }
}

/// Exact-owner application observed after executing a lifecycle body.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MainDocumentLifecycleTargetEffect {
    NotApplied {
        reason: MainDocumentLifecycleTargetRejection,
        current_owner: Option<FrameDocumentTaskOwner>,
    },
    Applied {
        current_owner_after_execution: Option<FrameDocumentTaskOwner>,
    },
}

/// How far execution entered lifecycle callback helpers.
///
/// Each `*Attempted` variant deliberately does not claim that every listener
/// ran: the existing best-effort helpers can record a warning after event
/// construction or dispatch fails. The window-load action needs two stages
/// because replacement can occur after complete/readystatechange but before
/// the compound load/pageshow helper.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MainDocumentLifecycleCallbackEffect {
    NotEntered,
    InteractiveReadystatechangeAttempted,
    DomContentLoadedAttempted,
    CompleteReadystatechangeAttempted,
    WindowLoadCompoundAttempted,
}

/// Best-effort settlement of one concrete lifecycle event body.
///
/// Failures have already been recorded as runtime warnings. The distinction
/// remains explicit because a failed DCL body must not publish a successful
/// event-end timing permit, while the selected task still owes its checkpoint.
#[must_use = "lifecycle event dispatch settlement determines the exact continuation"]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MainDocumentLifecycleEventDispatch {
    Completed,
    FailedBestEffort,
}

/// Propagated body settlement, kept orthogonal to callback entry.
///
/// A failure can occur after a callback was entered (for example while
/// publishing interactive image/media follow-up), so it must not be collapsed
/// into a target or callback boolean.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum MainDocumentLifecycleSettlement {
    Completed,
    Failed(String),
}

/// Concrete non-lifecycle work produced by a lifecycle body.
#[derive(Debug)]
pub(crate) enum MainDocumentLifecycleFollowup {
    None,
    ScheduleInternalLoading {
        task: PageOwnedInternalLoadingTask,
        ready_at: Instant,
    },
}

/// One lifecycle-specific checkpoint still owed by an executing ordinary
/// lifecycle task.
///
/// These variants are produced only after their preceding callback body has
/// run. They are not queue entries and cannot outlive the owner-local task.
/// Keeping the exact continuation in the variant prevents a generic
/// `already_checkpointed` flag from erasing whether the checkpoint separates
/// complete from load, load from pageshow, or ends the selected task.
#[must_use = "a lifecycle checkpoint must be performed and its exact continuation resumed"]
#[derive(Debug)]
pub(crate) struct MainDocumentLifecycleCheckpoint {
    execution: MainDocumentLifecycleExecution,
    continuation: MainDocumentLifecycleCheckpointContinuation,
}

#[derive(Debug)]
pub(super) enum MainDocumentLifecycleCheckpointContinuation {
    /// Final task-end after interactive `readystatechange`.
    FinishInteractive { owner: FrameDocumentTaskOwner },
    /// Final task-end after DOMContentLoaded.
    FinishDomContentLoaded {
        owner: FrameDocumentTaskOwner,
        event_end: MainDocumentLifecycleDomContentLoadedEventEnd,
    },
    /// Internal lifecycle boundary between complete/readystatechange and load.
    ContinueWindowLoad { owner: FrameDocumentTaskOwner },
    /// Internal lifecycle boundary between load and pageshow.
    ContinueWindowPageshow { owner: FrameDocumentTaskOwner },
    /// Final task-end after pageshow, or after a failed load/pageshow body.
    FinishWindowLoad { owner: FrameDocumentTaskOwner },
    /// A current-owner task was consumed without entering a callback body.
    FinishCurrentTaskWithoutCallback,
}

/// Whether successful DCL dispatch produced an event-end timing permit.
///
/// Dispatch failures remain best-effort warnings. They still owe the selected
/// task checkpoint, but must not manufacture a successful event-end fact.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum MainDocumentLifecycleDomContentLoadedEventEnd {
    Record,
    DispatchFailed,
}

/// Current execution state of one main-document lifecycle body.
#[must_use = "lifecycle execution must run every typed checkpoint continuation"]
#[derive(Debug)]
pub(crate) enum MainDocumentLifecycleStep {
    Checkpoint(MainDocumentLifecycleCheckpoint),
    Completed(MainDocumentLifecycleExecution),
}

/// Execution fact produced by exactly one main-document lifecycle body.
#[must_use = "main-document lifecycle execution must be reconciled by its PageVm coordinator"]
#[derive(Debug)]
pub(crate) struct MainDocumentLifecycleExecution {
    body: MainDocumentLifecycleBody,
    target: MainDocumentLifecycleTargetEffect,
    callback: MainDocumentLifecycleCallbackEffect,
    settlement: MainDocumentLifecycleSettlement,
    followup: MainDocumentLifecycleFollowup,
}

/// Successfully settled lifecycle execution accepted by the PageVm
/// coordinator. Failed executions cannot be constructed as this type, so a
/// caller cannot accidentally publish their follow-up in release builds.
#[must_use = "completed lifecycle execution must publish or discard its concrete follow-up"]
#[derive(Debug)]
pub(crate) struct MainDocumentLifecycleCompletion {
    body: MainDocumentLifecycleBody,
    target: MainDocumentLifecycleTargetEffect,
    callback: MainDocumentLifecycleCallbackEffect,
    followup: MainDocumentLifecycleFollowup,
}

/// Failed lifecycle execution with target and callback facts preserved.
#[derive(Debug)]
pub(crate) struct MainDocumentLifecycleFailure {
    body: MainDocumentLifecycleBody,
    target: MainDocumentLifecycleTargetEffect,
    callback: MainDocumentLifecycleCallbackEffect,
    message: String,
}

impl MainDocumentLifecycleExecution {
    pub(super) fn completed_not_applied(
        body: MainDocumentLifecycleBody,
        reason: MainDocumentLifecycleTargetRejection,
        current_owner: Option<FrameDocumentTaskOwner>,
    ) -> Self {
        Self {
            body,
            target: MainDocumentLifecycleTargetEffect::NotApplied {
                reason,
                current_owner,
            },
            callback: MainDocumentLifecycleCallbackEffect::NotEntered,
            settlement: MainDocumentLifecycleSettlement::Completed,
            followup: MainDocumentLifecycleFollowup::None,
        }
    }

    pub(super) fn applied(
        body: MainDocumentLifecycleBody,
        current_owner_after_execution: Option<FrameDocumentTaskOwner>,
        callback: MainDocumentLifecycleCallbackEffect,
        settlement: MainDocumentLifecycleSettlement,
        followup: MainDocumentLifecycleFollowup,
    ) -> Self {
        Self {
            body,
            target: MainDocumentLifecycleTargetEffect::Applied {
                current_owner_after_execution,
            },
            callback,
            settlement,
            followup,
        }
    }

    pub(super) fn checkpoint(
        self,
        continuation: MainDocumentLifecycleCheckpointContinuation,
    ) -> MainDocumentLifecycleStep {
        MainDocumentLifecycleStep::Checkpoint(MainDocumentLifecycleCheckpoint {
            execution: self,
            continuation,
        })
    }

    pub(super) fn completed(self) -> MainDocumentLifecycleStep {
        MainDocumentLifecycleStep::Completed(self)
    }

    pub(super) fn observe_current_owner_after_execution(
        &mut self,
        current_owner_after_execution: Option<FrameDocumentTaskOwner>,
    ) {
        if let MainDocumentLifecycleTargetEffect::Applied {
            current_owner_after_execution: observed,
        } = &mut self.target
        {
            *observed = current_owner_after_execution;
        }
    }

    pub(super) fn set_callback(&mut self, callback: MainDocumentLifecycleCallbackEffect) {
        self.callback = callback;
    }

    pub(super) fn set_followup(&mut self, followup: MainDocumentLifecycleFollowup) {
        self.followup = followup;
    }

    pub(super) fn fail(&mut self, message: String) {
        self.settlement = MainDocumentLifecycleSettlement::Failed(message);
        self.followup = MainDocumentLifecycleFollowup::None;
    }

    pub(crate) fn into_completion(
        self,
    ) -> Result<MainDocumentLifecycleCompletion, MainDocumentLifecycleFailure> {
        match self.settlement {
            MainDocumentLifecycleSettlement::Completed => Ok(MainDocumentLifecycleCompletion {
                body: self.body,
                target: self.target,
                callback: self.callback,
                followup: self.followup,
            }),
            MainDocumentLifecycleSettlement::Failed(message) => Err(MainDocumentLifecycleFailure {
                body: self.body,
                target: self.target,
                callback: self.callback,
                message,
            }),
        }
    }

    /// Compatibility adapter for direct ScriptVm tests that predate the
    /// PageVm lifecycle coordinator. Production PageVm execution consumes the
    /// typed fact directly and must not call this adapter.
    #[cfg(test)]
    pub(crate) fn into_legacy_post_parse_outcome(
        self,
    ) -> Result<NonScriptPageTaskExecutionOutcome, String> {
        match self.into_completion() {
            Err(failure) => Err(failure.into_message()),
            Ok(completion) => match completion.into_followup() {
                MainDocumentLifecycleFollowup::None => Ok(NonScriptPageTaskExecutionOutcome::None),
                MainDocumentLifecycleFollowup::ScheduleInternalLoading { task, ready_at } => {
                    Ok(NonScriptPageTaskExecutionOutcome::ScheduleInternalLoading {
                        task,
                        ready_at,
                    })
                }
            },
        }
    }
}

impl MainDocumentLifecycleCheckpoint {
    pub(super) fn into_parts(
        self,
    ) -> (
        MainDocumentLifecycleExecution,
        MainDocumentLifecycleCheckpointContinuation,
    ) {
        (self.execution, self.continuation)
    }
}

impl MainDocumentLifecycleCompletion {
    pub(crate) fn not_applied(
        body: MainDocumentLifecycleBody,
        reason: MainDocumentLifecycleTargetRejection,
        current_owner: Option<FrameDocumentTaskOwner>,
    ) -> Self {
        Self {
            body,
            target: MainDocumentLifecycleTargetEffect::NotApplied {
                reason,
                current_owner,
            },
            callback: MainDocumentLifecycleCallbackEffect::NotEntered,
            followup: MainDocumentLifecycleFollowup::None,
        }
    }

    pub(crate) fn skipped_for_pending_navigation(
        body: MainDocumentLifecycleBody,
        current_owner: Option<FrameDocumentTaskOwner>,
    ) -> Self {
        Self::not_applied(
            body,
            MainDocumentLifecycleTargetRejection::PendingCrossDocumentNavigation,
            current_owner,
        )
    }

    pub(crate) const fn kind(&self) -> MainDocumentLifecycleBodyKind {
        self.body.kind()
    }

    pub(crate) fn owner(&self) -> FrameDocumentTaskOwner {
        self.body.owner()
    }

    pub(crate) const fn target(&self) -> MainDocumentLifecycleTargetEffect {
        self.target
    }

    pub(crate) const fn callback(&self) -> MainDocumentLifecycleCallbackEffect {
        self.callback
    }

    pub(crate) fn into_followup(self) -> MainDocumentLifecycleFollowup {
        self.followup
    }
}

impl MainDocumentLifecycleFailure {
    pub(crate) const fn kind(&self) -> MainDocumentLifecycleBodyKind {
        self.body.kind()
    }

    pub(crate) fn owner(&self) -> FrameDocumentTaskOwner {
        self.body.owner()
    }

    pub(crate) const fn target(&self) -> MainDocumentLifecycleTargetEffect {
        self.target
    }

    pub(crate) const fn callback(&self) -> MainDocumentLifecycleCallbackEffect {
        self.callback
    }

    pub(crate) fn into_message(self) -> String {
        self.message
    }
}
