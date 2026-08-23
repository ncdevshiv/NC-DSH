//! Runtime-independent `ReadableStream` tee coordination.
//!
//! Branch wrappers, JavaScript chunks and cancel reasons, promises, BYOB
//! views, and ArrayBuffer cloning remain adapter-owned. This module receives
//! primitive snapshots and decides default/byte routing, demand, cancellation,
//! and terminal propagation.

use crate::readable::ReadableState;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TeeKind {
    Default,
    Byte,
}

impl TeeKind {
    #[must_use]
    pub const fn from_byte_stream(byte_stream: bool) -> Self {
        if byte_stream {
            Self::Byte
        } else {
            Self::Default
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TeeBranch {
    First,
    Second,
}

impl TeeBranch {
    #[must_use]
    pub const fn from_index(index: u32) -> Self {
        if index == 0 {
            Self::First
        } else {
            Self::Second
        }
    }

    #[must_use]
    pub const fn index(self) -> u32 {
        match self {
            Self::First => 0,
            Self::Second => 1,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BranchPair<T> {
    first: T,
    second: T,
}

impl<T> BranchPair<T> {
    #[must_use]
    pub const fn new(first: T, second: T) -> Self {
        Self { first, second }
    }
}

impl<T: Copy> BranchPair<T> {
    #[must_use]
    pub const fn get(self, branch: TeeBranch) -> T {
        match branch {
            TeeBranch::First => self.first,
            TeeBranch::Second => self.second,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TeeBranchSnapshot {
    present: bool,
    canceled: bool,
    state: ReadableState,
    close_requested: bool,
}

impl TeeBranchSnapshot {
    #[must_use]
    pub const fn new(
        present: bool,
        canceled: bool,
        state: ReadableState,
        close_requested: bool,
    ) -> Self {
        Self {
            present,
            canceled,
            state,
            close_requested,
        }
    }

    #[must_use]
    pub const fn missing(canceled: bool) -> Self {
        Self::new(false, canceled, ReadableState::Closed, false)
    }

    const fn terminal(self) -> bool {
        !matches!(self.state, ReadableState::Readable) || self.close_requested
    }

    const fn accepts_default_chunk(self) -> bool {
        self.present && !self.terminal()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TeeSnapshot {
    kind: TeeKind,
    source_state: ReadableState,
    source_close_requested: bool,
    branches: BranchPair<TeeBranchSnapshot>,
    cancel_settled: bool,
    reading: bool,
    read_again: BranchPair<bool>,
    byob_owner: Option<TeeBranch>,
}

impl TeeSnapshot {
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn new(
        kind: TeeKind,
        source_state: ReadableState,
        source_close_requested: bool,
        branches: BranchPair<TeeBranchSnapshot>,
        cancel_settled: bool,
        reading: bool,
        byob_owner: Option<TeeBranch>,
    ) -> Self {
        Self {
            kind,
            source_state,
            source_close_requested,
            branches,
            cancel_settled,
            reading,
            read_again: BranchPair::new(false, false),
            byob_owner,
        }
    }

    /// Supplies the live per-branch demand recorded while one tee read owns
    /// the source. Default tees collapse the pair to one logical flag; byte
    /// tees retain the requesting branch so the next BYOB owner is stable.
    #[must_use]
    pub const fn with_read_again(mut self, read_again: BranchPair<bool>) -> Self {
        self.read_again = read_again;
        self
    }

    #[must_use]
    pub const fn plan_start(self) -> TeeStartPlan {
        if matches!(self.source_state, ReadableState::Errored) {
            return TeeStartPlan::ErrorBranches;
        }
        match self.kind {
            TeeKind::Byte if matches!(self.source_state, ReadableState::Closed) => {
                TeeStartPlan::CloseByteBranches
            }
            TeeKind::Byte => TeeStartPlan::WaitForByteBranchDemand,
            TeeKind::Default if matches!(self.source_state, ReadableState::Closed) => {
                TeeStartPlan::CloseDefaultBranches
            }
            TeeKind::Default => TeeStartPlan::WaitForDefaultBranchStarts,
        }
    }

    /// A close request can coexist with queued source chunks, so it is
    /// observable metadata but is deliberately not treated as terminal.
    #[must_use]
    pub const fn source_close_requested(self) -> bool {
        self.source_close_requested
    }

    #[must_use]
    pub const fn plan_branch_pull(self, branch: TeeBranch) -> TeeBranchPullPlan {
        let branch_state = self.branches.get(branch);
        if !branch_state.present || branch_state.canceled {
            return TeeBranchPullPlan::Ignore;
        }
        // An in-flight read owns distribution even if the source published a
        // terminal state after producing its chunk. Closing branches here
        // would let the first branch's enqueue preempt delivery to the second.
        if self.reading {
            return TeeBranchPullPlan::RecordReadAgain { branch };
        }
        if matches!(self.source_state, ReadableState::Errored) {
            return TeeBranchPullPlan::Ignore;
        }
        if matches!(self.source_state, ReadableState::Closed) {
            return TeeBranchPullPlan::CloseBranches;
        }
        match self.kind {
            TeeKind::Default => TeeBranchPullPlan::StartDefaultRead,
            TeeKind::Byte => TeeBranchPullPlan::InspectByteReadMode { branch },
        }
    }

    #[must_use]
    pub const fn plan_byte_read_start(
        self,
        branch: TeeBranch,
        has_pending_byob_view: bool,
    ) -> Option<ByteReadStartPlan> {
        if !matches!(
            self.plan_branch_pull(branch),
            TeeBranchPullPlan::InspectByteReadMode { .. }
        ) {
            return None;
        }
        Some(ByteReadStartPlan {
            branch,
            mode: if has_pending_byob_view {
                ByteReadMode::Byob
            } else {
                ByteReadMode::Default
            },
        })
    }

    #[must_use]
    pub const fn plan_default_chunk(self) -> BranchPair<DefaultChunkAction> {
        BranchPair::new(
            if self.branches.first.accepts_default_chunk() {
                DefaultChunkAction::Enqueue
            } else {
                DefaultChunkAction::Skip
            },
            if self.branches.second.accepts_default_chunk() {
                DefaultChunkAction::Enqueue
            } else {
                DefaultChunkAction::Skip
            },
        )
    }

    #[must_use]
    pub const fn plan_default_read_fulfilled(
        self,
        result: DefaultReadResultSnapshot,
    ) -> DefaultReadFulfillmentPlan {
        if !result.valid_result {
            return DefaultReadFulfillmentPlan::InvalidResult;
        }
        if result.done {
            return DefaultReadFulfillmentPlan::CloseBranches {
                branches: self.present_terminal_actions(TerminalBranchAction::Close),
                settle_cancel: !self.cancel_settled,
            };
        }
        DefaultReadFulfillmentPlan::Distribute {
            branches: self.plan_default_chunk(),
        }
    }

    /// Must be evaluated from a fresh snapshot after both branch enqueue
    /// effects. Those effects can synchronously request another tee read.
    #[must_use]
    pub const fn plan_after_default_distribution(
        self,
        source_closed: bool,
    ) -> DefaultDistributionContinuation {
        if source_closed {
            return DefaultDistributionContinuation::CloseBranches;
        }
        if self.read_again.first || self.read_again.second {
            DefaultDistributionContinuation::StartRead
        } else {
            DefaultDistributionContinuation::Idle
        }
    }

    #[must_use]
    pub const fn plan_source_close(self) -> SourceClosePlan {
        if self.reading {
            return SourceClosePlan::WaitForReadReaction;
        }
        match self.kind {
            TeeKind::Byte => SourceClosePlan::WaitForReadReaction,
            TeeKind::Default => SourceClosePlan::CloseDefaultBranches {
                branches: self.present_terminal_actions(TerminalBranchAction::Close),
                settle_cancel: !self.cancel_settled,
            },
        }
    }

    #[must_use]
    pub const fn plan_source_error(self) -> SourceErrorPlan {
        SourceErrorPlan::ErrorBranches {
            branches: self.present_terminal_actions(TerminalBranchAction::Error),
            settle_cancel: !self.cancel_settled,
        }
    }

    #[must_use]
    pub const fn plan_branch_cancel(self, branch: TeeBranch) -> BranchCancelPlan {
        let first_canceled = self.branches.first.canceled || matches!(branch, TeeBranch::First);
        let second_canceled = self.branches.second.canceled || matches!(branch, TeeBranch::Second);
        if first_canceled && second_canceled && !self.cancel_settled {
            BranchCancelPlan::RecordReasonAndCancelSource
        } else {
            BranchCancelPlan::RecordReasonAndWait
        }
    }

    #[must_use]
    pub const fn plan_settle_cancel(self) -> CancelSettlementPlan {
        if self.cancel_settled {
            CancelSettlementPlan::AlreadySettled
        } else {
            CancelSettlementPlan::MarkSettledAndResolve
        }
    }

    #[must_use]
    pub const fn plan_byte_read_fulfilled(
        self,
        result: ByteReadResultSnapshot,
    ) -> ByteReadFulfillmentPlan {
        if !result.valid_result {
            return ByteReadFulfillmentPlan::Error(ByteReadFailure::InvalidResult);
        }
        if result.done {
            return ByteReadFulfillmentPlan::CloseBranches {
                branches: self.byte_close_actions(self.byob_owner),
                settle_cancel: !self.cancel_settled,
            };
        }
        if !result.has_value {
            return ByteReadFulfillmentPlan::Error(ByteReadFailure::MissingChunk);
        }
        if !result.value_is_bytes {
            return ByteReadFulfillmentPlan::Error(ByteReadFailure::ChunkIsNotBytes);
        }
        ByteReadFulfillmentPlan::Distribute {
            branches: BranchPair::new(
                self.byte_chunk_action(TeeBranch::First),
                self.byte_chunk_action(TeeBranch::Second),
            ),
        }
    }

    /// Re-decode the source after branch distribution before selecting the
    /// continuation because adapter effects may synchronously change it.
    #[must_use]
    pub const fn plan_after_byte_distribution(
        self,
        source_closed: bool,
    ) -> ByteDistributionContinuation {
        if source_closed {
            ByteDistributionContinuation::CloseBranches
        } else if self.read_again.first {
            ByteDistributionContinuation::PullBranch(TeeBranch::First)
        } else if self.read_again.second {
            ByteDistributionContinuation::PullBranch(TeeBranch::Second)
        } else {
            ByteDistributionContinuation::Idle
        }
    }

    #[must_use]
    pub const fn plan_byte_read_rejected(self) -> ByteReadRejectionPlan {
        ByteReadRejectionPlan {
            branches: self.present_terminal_actions(TerminalBranchAction::Error),
            settle_cancel: !self.cancel_settled,
        }
    }

    #[must_use]
    pub const fn plan_byte_close(self, terminal_byob_owner: Option<TeeBranch>) -> ByteClosePlan {
        ByteClosePlan {
            branches: self.byte_close_actions(terminal_byob_owner),
            settle_cancel: !self.cancel_settled,
        }
    }

    const fn present_terminal_actions(
        self,
        action: TerminalBranchAction,
    ) -> BranchPair<TerminalBranchAction> {
        BranchPair::new(
            if self.branches.first.present {
                action
            } else {
                TerminalBranchAction::Skip
            },
            if self.branches.second.present {
                action
            } else {
                TerminalBranchAction::Skip
            },
        )
    }

    const fn byte_chunk_action(self, branch: TeeBranch) -> ByteChunkAction {
        let state = self.branches.get(branch);
        if !state.present || state.canceled {
            ByteChunkAction::Skip
        } else if byob_owner_matches(self.byob_owner, branch) {
            ByteChunkAction::RespondWithOriginalView
        } else if self.byob_owner.is_none() && matches!(branch, TeeBranch::First) {
            ByteChunkAction::EnqueueOriginalView
        } else {
            ByteChunkAction::EnqueueClonedBytes
        }
    }

    const fn byte_close_actions(
        self,
        terminal_byob_owner: Option<TeeBranch>,
    ) -> BranchPair<ByteCloseAction> {
        BranchPair::new(
            self.byte_close_action(TeeBranch::First, terminal_byob_owner),
            self.byte_close_action(TeeBranch::Second, terminal_byob_owner),
        )
    }

    const fn byte_close_action(
        self,
        branch: TeeBranch,
        terminal_byob_owner: Option<TeeBranch>,
    ) -> ByteCloseAction {
        let state = self.branches.get(branch);
        if !state.present || state.canceled {
            ByteCloseAction::Skip
        } else if byob_owner_matches(terminal_byob_owner, branch) {
            ByteCloseAction::CloseAndRespondWithView
        } else {
            ByteCloseAction::CloseAndFinish
        }
    }
}

const fn byob_owner_matches(owner: Option<TeeBranch>, branch: TeeBranch) -> bool {
    matches!(
        (owner, branch),
        (Some(TeeBranch::First), TeeBranch::First) | (Some(TeeBranch::Second), TeeBranch::Second)
    )
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TeeEntrySnapshot {
    source_locked: bool,
}

impl TeeEntrySnapshot {
    #[must_use]
    pub const fn new(source_locked: bool) -> Self {
        Self { source_locked }
    }

    #[must_use]
    pub const fn plan(self) -> TeeEntryPlan {
        if self.source_locked {
            TeeEntryPlan::RejectLocked
        } else {
            TeeEntryPlan::Start
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TeeEntryPlan {
    Start,
    RejectLocked,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TeeStartPlan {
    ErrorBranches,
    CloseByteBranches,
    CloseDefaultBranches,
    WaitForDefaultBranchStarts,
    WaitForByteBranchDemand,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TeeBranchPullPlan {
    Ignore,
    CloseBranches,
    RecordReadAgain { branch: TeeBranch },
    StartDefaultRead,
    InspectByteReadMode { branch: TeeBranch },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ByteReadStartPlan {
    branch: TeeBranch,
    mode: ByteReadMode,
}

impl ByteReadStartPlan {
    #[must_use]
    pub const fn branch(self) -> TeeBranch {
        self.branch
    }

    #[must_use]
    pub const fn mode(self) -> ByteReadMode {
        self.mode
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ByteReadMode {
    Default,
    Byob,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DefaultChunkAction {
    Skip,
    Enqueue,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DefaultReadResultSnapshot {
    valid_result: bool,
    done: bool,
}

impl DefaultReadResultSnapshot {
    #[must_use]
    pub const fn new(valid_result: bool, done: bool) -> Self {
        Self { valid_result, done }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DefaultReadFulfillmentPlan {
    InvalidResult,
    CloseBranches {
        branches: BranchPair<TerminalBranchAction>,
        settle_cancel: bool,
    },
    Distribute {
        branches: BranchPair<DefaultChunkAction>,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DefaultDistributionContinuation {
    Idle,
    StartRead,
    CloseBranches,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SourceClosePlan {
    WaitForReadReaction,
    CloseDefaultBranches {
        branches: BranchPair<TerminalBranchAction>,
        settle_cancel: bool,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SourceErrorPlan {
    ErrorBranches {
        branches: BranchPair<TerminalBranchAction>,
        settle_cancel: bool,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminalBranchAction {
    Skip,
    Close,
    Error,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BranchCancelPlan {
    RecordReasonAndWait,
    RecordReasonAndCancelSource,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CancelSettlementPlan {
    AlreadySettled,
    MarkSettledAndResolve,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ByteReadResultSnapshot {
    valid_result: bool,
    done: bool,
    has_value: bool,
    value_is_bytes: bool,
}

impl ByteReadResultSnapshot {
    #[must_use]
    pub const fn new(
        valid_result: bool,
        done: bool,
        has_value: bool,
        value_is_bytes: bool,
    ) -> Self {
        Self {
            valid_result,
            done,
            has_value,
            value_is_bytes,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ByteReadFulfillmentPlan {
    Error(ByteReadFailure),
    CloseBranches {
        branches: BranchPair<ByteCloseAction>,
        settle_cancel: bool,
    },
    Distribute {
        branches: BranchPair<ByteChunkAction>,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ByteReadFailure {
    InvalidResult,
    MissingChunk,
    ChunkIsNotBytes,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ByteChunkAction {
    Skip,
    RespondWithOriginalView,
    EnqueueOriginalView,
    EnqueueClonedBytes,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ByteDistributionContinuation {
    Idle,
    PullBranch(TeeBranch),
    CloseBranches,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ByteReadRejectionPlan {
    branches: BranchPair<TerminalBranchAction>,
    settle_cancel: bool,
}

impl ByteReadRejectionPlan {
    #[must_use]
    pub const fn branches(self) -> BranchPair<TerminalBranchAction> {
        self.branches
    }

    #[must_use]
    pub const fn settle_cancel(self) -> bool {
        self.settle_cancel
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ByteClosePlan {
    branches: BranchPair<ByteCloseAction>,
    settle_cancel: bool,
}

impl ByteClosePlan {
    #[must_use]
    pub const fn branches(self) -> BranchPair<ByteCloseAction> {
        self.branches
    }

    #[must_use]
    pub const fn settle_cancel(self) -> bool {
        self.settle_cancel
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ByteCloseAction {
    Skip,
    CloseAndFinish,
    CloseAndRespondWithView,
}

#[cfg(test)]
mod tests {
    use super::*;

    const OPEN: TeeBranchSnapshot =
        TeeBranchSnapshot::new(true, false, ReadableState::Readable, false);

    fn snapshot(kind: TeeKind) -> TeeSnapshot {
        TeeSnapshot::new(
            kind,
            ReadableState::Readable,
            false,
            BranchPair::new(OPEN, OPEN),
            false,
            false,
            None,
        )
    }

    #[test]
    fn entry_and_start_plans_partition_kind_and_source_terminal_state() {
        assert_eq!(TeeEntrySnapshot::new(false).plan(), TeeEntryPlan::Start);
        assert_eq!(
            TeeEntrySnapshot::new(true).plan(),
            TeeEntryPlan::RejectLocked
        );
        assert_eq!(
            snapshot(TeeKind::Default).plan_start(),
            TeeStartPlan::WaitForDefaultBranchStarts
        );
        assert_eq!(
            snapshot(TeeKind::Byte).plan_start(),
            TeeStartPlan::WaitForByteBranchDemand
        );
        let errored = TeeSnapshot::new(
            TeeKind::Byte,
            ReadableState::Errored,
            false,
            BranchPair::new(OPEN, OPEN),
            false,
            false,
            None,
        );
        assert_eq!(errored.plan_start(), TeeStartPlan::ErrorBranches);
    }

    #[test]
    fn branch_pull_selects_default_byob_and_in_flight_routes() {
        assert_eq!(
            snapshot(TeeKind::Default).plan_branch_pull(TeeBranch::Second),
            TeeBranchPullPlan::StartDefaultRead
        );
        let byob = TeeBranchSnapshot::new(true, false, ReadableState::Readable, false);
        let byte = TeeSnapshot::new(
            TeeKind::Byte,
            ReadableState::Readable,
            false,
            BranchPair::new(byob, OPEN),
            false,
            false,
            None,
        );
        assert_eq!(
            byte.plan_branch_pull(TeeBranch::First),
            TeeBranchPullPlan::InspectByteReadMode {
                branch: TeeBranch::First,
            }
        );
        assert_eq!(
            byte.plan_byte_read_start(TeeBranch::First, true),
            Some(ByteReadStartPlan {
                branch: TeeBranch::First,
                mode: ByteReadMode::Byob,
            })
        );
        let reading = TeeSnapshot::new(
            TeeKind::Byte,
            ReadableState::Readable,
            false,
            BranchPair::new(byob, OPEN),
            false,
            true,
            Some(TeeBranch::First),
        );
        assert_eq!(
            reading.plan_branch_pull(TeeBranch::Second),
            TeeBranchPullPlan::RecordReadAgain {
                branch: TeeBranch::Second,
            }
        );

        let terminal = |state, reading| {
            TeeSnapshot::new(
                TeeKind::Byte,
                state,
                false,
                BranchPair::new(OPEN, OPEN),
                false,
                reading,
                None,
            )
        };
        assert_eq!(
            terminal(ReadableState::Closed, false).plan_branch_pull(TeeBranch::First),
            TeeBranchPullPlan::CloseBranches
        );
        assert_eq!(
            terminal(ReadableState::Errored, false).plan_branch_pull(TeeBranch::First),
            TeeBranchPullPlan::Ignore
        );
        assert_eq!(
            terminal(ReadableState::Closed, true).plan_branch_pull(TeeBranch::Second),
            TeeBranchPullPlan::RecordReadAgain {
                branch: TeeBranch::Second,
            }
        );

        let draining = TeeSnapshot::new(
            TeeKind::Default,
            ReadableState::Readable,
            true,
            BranchPair::new(OPEN, OPEN),
            false,
            false,
            None,
        );
        assert!(draining.source_close_requested());
        assert_eq!(
            draining.plan_branch_pull(TeeBranch::First),
            TeeBranchPullPlan::StartDefaultRead
        );
    }

    #[test]
    fn default_chunk_and_source_terminal_plans_are_branch_typed() {
        let closed = TeeBranchSnapshot::new(true, false, ReadableState::Closed, false);
        let current = TeeSnapshot::new(
            TeeKind::Default,
            ReadableState::Readable,
            false,
            BranchPair::new(OPEN, closed),
            false,
            false,
            None,
        );
        assert_eq!(
            current.plan_default_chunk(),
            BranchPair::new(DefaultChunkAction::Enqueue, DefaultChunkAction::Skip)
        );
        assert_eq!(
            current.plan_source_close(),
            SourceClosePlan::CloseDefaultBranches {
                branches: BranchPair::new(TerminalBranchAction::Close, TerminalBranchAction::Close,),
                settle_cancel: true,
            }
        );
        assert_eq!(
            current.plan_default_read_fulfilled(DefaultReadResultSnapshot::new(true, false)),
            DefaultReadFulfillmentPlan::Distribute {
                branches: BranchPair::new(DefaultChunkAction::Enqueue, DefaultChunkAction::Skip,),
            }
        );
        assert_eq!(
            current.plan_default_read_fulfilled(DefaultReadResultSnapshot::new(true, true)),
            DefaultReadFulfillmentPlan::CloseBranches {
                branches: BranchPair::new(TerminalBranchAction::Close, TerminalBranchAction::Close,),
                settle_cancel: true,
            }
        );
        assert_eq!(
            current.plan_default_read_fulfilled(DefaultReadResultSnapshot::new(false, false)),
            DefaultReadFulfillmentPlan::InvalidResult
        );

        let demand_while_reading = current.with_read_again(BranchPair::new(false, true));
        assert_eq!(
            demand_while_reading.plan_after_default_distribution(false),
            DefaultDistributionContinuation::StartRead
        );
        assert_eq!(
            current.plan_after_default_distribution(false),
            DefaultDistributionContinuation::Idle
        );
        assert_eq!(
            demand_while_reading.plan_after_default_distribution(true),
            DefaultDistributionContinuation::CloseBranches
        );
        let reading = TeeSnapshot::new(
            TeeKind::Default,
            ReadableState::Readable,
            false,
            BranchPair::new(OPEN, OPEN),
            false,
            true,
            None,
        );
        assert_eq!(
            reading.plan_source_close(),
            SourceClosePlan::WaitForReadReaction
        );
    }

    #[test]
    fn cancel_waits_for_both_reasons_and_settles_only_once() {
        let current = snapshot(TeeKind::Default);
        assert_eq!(
            current.plan_branch_cancel(TeeBranch::First),
            BranchCancelPlan::RecordReasonAndWait
        );
        let first_canceled = TeeBranchSnapshot::new(true, true, ReadableState::Closed, false);
        let second = TeeSnapshot::new(
            TeeKind::Default,
            ReadableState::Readable,
            false,
            BranchPair::new(first_canceled, OPEN),
            false,
            false,
            None,
        );
        assert_eq!(
            second.plan_branch_cancel(TeeBranch::Second),
            BranchCancelPlan::RecordReasonAndCancelSource
        );
        assert_eq!(
            second.plan_settle_cancel(),
            CancelSettlementPlan::MarkSettledAndResolve
        );
    }

    #[test]
    fn byte_result_validation_precedes_distribution() {
        let current = snapshot(TeeKind::Byte);
        assert_eq!(
            current
                .plan_byte_read_fulfilled(ByteReadResultSnapshot::new(false, false, false, false,)),
            ByteReadFulfillmentPlan::Error(ByteReadFailure::InvalidResult)
        );
        assert_eq!(
            current
                .plan_byte_read_fulfilled(ByteReadResultSnapshot::new(true, false, false, false,)),
            ByteReadFulfillmentPlan::Error(ByteReadFailure::MissingChunk)
        );
        assert_eq!(
            current
                .plan_byte_read_fulfilled(ByteReadResultSnapshot::new(true, false, true, false,)),
            ByteReadFulfillmentPlan::Error(ByteReadFailure::ChunkIsNotBytes)
        );
    }

    #[test]
    fn byte_distribution_preserves_byob_owner_and_clones_other_branch() {
        let current = TeeSnapshot::new(
            TeeKind::Byte,
            ReadableState::Readable,
            false,
            BranchPair::new(OPEN, OPEN),
            false,
            true,
            Some(TeeBranch::Second),
        );
        assert_eq!(
            current.plan_byte_read_fulfilled(ByteReadResultSnapshot::new(true, false, true, true,)),
            ByteReadFulfillmentPlan::Distribute {
                branches: BranchPair::new(
                    ByteChunkAction::EnqueueClonedBytes,
                    ByteChunkAction::RespondWithOriginalView,
                ),
            }
        );
        assert_eq!(
            current.plan_after_byte_distribution(false),
            ByteDistributionContinuation::Idle
        );
        assert_eq!(
            current
                .with_read_again(BranchPair::new(false, true))
                .plan_after_byte_distribution(false),
            ByteDistributionContinuation::PullBranch(TeeBranch::Second)
        );
        assert_eq!(
            current
                .with_read_again(BranchPair::new(true, true))
                .plan_after_byte_distribution(false),
            ByteDistributionContinuation::PullBranch(TeeBranch::First)
        );
        assert_eq!(
            current
                .with_read_again(BranchPair::new(true, true))
                .plan_after_byte_distribution(true),
            ByteDistributionContinuation::CloseBranches
        );
    }

    #[test]
    fn byte_distribution_exhaustively_routes_owner_and_canceled_branches() {
        for first_canceled in [false, true] {
            for second_canceled in [false, true] {
                for owner in [None, Some(TeeBranch::First), Some(TeeBranch::Second)] {
                    let branch = |canceled| {
                        TeeBranchSnapshot::new(true, canceled, ReadableState::Readable, false)
                    };
                    let current = TeeSnapshot::new(
                        TeeKind::Byte,
                        ReadableState::Readable,
                        false,
                        BranchPair::new(branch(first_canceled), branch(second_canceled)),
                        false,
                        true,
                        owner,
                    );
                    let expected = |branch, canceled| {
                        if canceled {
                            ByteChunkAction::Skip
                        } else if byob_owner_matches(owner, branch) {
                            ByteChunkAction::RespondWithOriginalView
                        } else if owner.is_none() && matches!(branch, TeeBranch::First) {
                            ByteChunkAction::EnqueueOriginalView
                        } else {
                            ByteChunkAction::EnqueueClonedBytes
                        }
                    };
                    let ByteReadFulfillmentPlan::Distribute { branches } = current
                        .plan_byte_read_fulfilled(ByteReadResultSnapshot::new(
                            true, false, true, true,
                        ))
                    else {
                        panic!("valid bytes must produce a distribution plan")
                    };
                    assert_eq!(
                        branches,
                        BranchPair::new(
                            expected(TeeBranch::First, first_canceled),
                            expected(TeeBranch::Second, second_canceled),
                        )
                    );
                }
            }
        }
    }

    #[test]
    fn byte_done_routes_terminal_byob_view_to_its_owner() {
        let current = TeeSnapshot::new(
            TeeKind::Byte,
            ReadableState::Closed,
            false,
            BranchPair::new(OPEN, OPEN),
            false,
            true,
            Some(TeeBranch::First),
        );
        assert_eq!(
            current.plan_byte_read_fulfilled(ByteReadResultSnapshot::new(true, true, true, true,)),
            ByteReadFulfillmentPlan::CloseBranches {
                branches: BranchPair::new(
                    ByteCloseAction::CloseAndRespondWithView,
                    ByteCloseAction::CloseAndFinish,
                ),
                settle_cancel: true,
            }
        );
    }

    #[test]
    fn byte_close_and_error_skip_only_missing_or_canceled_branches() {
        let canceled = TeeBranchSnapshot::new(true, true, ReadableState::Closed, false);
        let current = TeeSnapshot::new(
            TeeKind::Byte,
            ReadableState::Readable,
            false,
            BranchPair::new(canceled, OPEN),
            false,
            false,
            None,
        );
        assert_eq!(
            current.plan_byte_close(None).branches(),
            BranchPair::new(ByteCloseAction::Skip, ByteCloseAction::CloseAndFinish)
        );
        assert_eq!(
            current.plan_byte_read_rejected().branches(),
            BranchPair::new(TerminalBranchAction::Error, TerminalBranchAction::Error)
        );
    }
}
