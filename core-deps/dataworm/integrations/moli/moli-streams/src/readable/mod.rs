//! Runtime-independent `ReadableStream` lifecycle decisions.

pub mod byte_controller;
pub mod default_controller;
pub mod iterator;

/// The externally meaningful lifecycle state. Runtime adapters retain the
/// actual stored error value separately.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReadableState {
    Readable,
    Closed,
    Errored,
}

impl ReadableState {
    /// Decodes the current V8/private-slot representation. An error wins over
    /// the closed bit because errored streams commonly store both.
    #[must_use]
    pub const fn from_storage(closed: bool, has_stored_error: bool) -> Self {
        if has_stored_error {
            Self::Errored
        } else if closed {
            Self::Closed
        } else {
            Self::Readable
        }
    }

    #[must_use]
    pub const fn is_readable(self) -> bool {
        matches!(self, Self::Readable)
    }
}

/// A point-in-time lifecycle snapshot decoded from adapter-owned storage.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReadableSnapshot {
    state: ReadableState,
    close_requested: bool,
    queue_empty: bool,
    pending_read_count: usize,
}

impl ReadableSnapshot {
    #[must_use]
    pub const fn new(
        state: ReadableState,
        close_requested: bool,
        queue_empty: bool,
        pending_read_count: usize,
    ) -> Self {
        Self {
            state,
            close_requested,
            queue_empty,
            pending_read_count,
        }
    }

    #[must_use]
    pub const fn state(self) -> ReadableState {
        self.state
    }

    #[must_use]
    pub const fn close_requested(self) -> bool {
        self.close_requested
    }

    #[must_use]
    pub const fn queue_empty(self) -> bool {
        self.queue_empty
    }

    #[must_use]
    pub const fn pending_read_count(self) -> usize {
        self.pending_read_count
    }

    #[must_use]
    pub const fn plan_enqueue(self) -> EnqueuePlan {
        if self.state.is_readable() && !self.close_requested {
            EnqueuePlan::Continue
        } else {
            EnqueuePlan::Reject
        }
    }

    #[must_use]
    pub const fn plan_request_close(self) -> CloseRequestPlan {
        if self.state.is_readable() && !self.close_requested {
            CloseRequestPlan::Request
        } else {
            CloseRequestPlan::Ignore
        }
    }

    #[must_use]
    pub const fn plan_finish_close(self) -> FinishClosePlan {
        if self.state.is_readable() && self.close_requested && self.queue_empty {
            FinishClosePlan::Finish
        } else {
            FinishClosePlan::Wait
        }
    }

    #[must_use]
    pub const fn plan_error(self) -> ErrorPlan {
        if self.state.is_readable() {
            ErrorPlan::Error
        } else {
            ErrorPlan::Ignore
        }
    }

    #[must_use]
    pub const fn plan_read_start(self) -> ReadStartPlan {
        match self.state {
            ReadableState::Readable => ReadStartPlan::Continue,
            ReadableState::Closed => ReadStartPlan::ResolveDone,
            ReadableState::Errored => ReadStartPlan::RejectStoredError,
        }
    }

    #[must_use]
    pub const fn plan_closed_promise(self) -> ClosedPromisePlan {
        match self.state {
            ReadableState::Readable => ClosedPromisePlan::Wait,
            ReadableState::Closed => ClosedPromisePlan::Resolve,
            ReadableState::Errored => ClosedPromisePlan::RejectStoredError,
        }
    }

    #[must_use]
    pub const fn plan_cancel(self) -> CancelPlan {
        match self.state {
            ReadableState::Closed => CancelPlan::Resolve,
            ReadableState::Errored => CancelPlan::RejectStoredError,
            ReadableState::Readable => CancelPlan::RunAlgorithm {
                finish_requested_close: self.close_requested,
            },
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EnqueuePlan {
    Continue,
    Reject,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CloseRequestPlan {
    Request,
    Ignore,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FinishClosePlan {
    Finish,
    Wait,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ErrorPlan {
    Error,
    Ignore,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReadStartPlan {
    Continue,
    ResolveDone,
    RejectStoredError,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClosedPromisePlan {
    Wait,
    Resolve,
    RejectStoredError,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CancelPlan {
    RunAlgorithm { finish_requested_close: bool },
    Resolve,
    RejectStoredError,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReadableKind {
    Default,
    Byte,
}

impl ReadableKind {
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
pub enum ReaderKind {
    Default,
    Byob,
}

/// Lock/disturbed flags are kept separate so callers that only need access
/// state do not have to decode queue or error storage.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReadableAccessSnapshot {
    locked: bool,
    disturbed: bool,
}

impl ReadableAccessSnapshot {
    #[must_use]
    pub const fn new(locked: bool, disturbed: bool) -> Self {
        Self { locked, disturbed }
    }

    #[must_use]
    pub const fn locked(self) -> bool {
        self.locked
    }

    #[must_use]
    pub const fn disturbed(self) -> bool {
        self.disturbed
    }

    #[must_use]
    pub const fn plan_lock(self) -> ReadableLockPlan {
        if self.locked {
            ReadableLockPlan::RejectLocked
        } else {
            ReadableLockPlan::Lock(self.locked_transition(true))
        }
    }

    /// Plan `ReadableStream.prototype.getReader()`. The public method checks
    /// the lock before rejecting a BYOB reader for a default stream, matching
    /// the renderer's existing observable error ordering.
    #[must_use]
    pub const fn plan_get_reader(
        self,
        reader: ReaderKind,
        stream: ReadableKind,
    ) -> AcquireReaderPlan {
        if self.locked {
            AcquireReaderPlan::RejectLocked
        } else if matches!((reader, stream), (ReaderKind::Byob, ReadableKind::Default)) {
            AcquireReaderPlan::RejectIncompatibleByob
        } else {
            AcquireReaderPlan::Acquire(self.locked_transition(true))
        }
    }

    /// Plan the exposed reader constructors. The BYOB constructor validates
    /// stream kind before lock state, which is intentionally distinct from the
    /// current `getReader()` entrypoint ordering above.
    #[must_use]
    pub const fn plan_reader_constructor(
        self,
        reader: ReaderKind,
        stream: ReadableKind,
    ) -> AcquireReaderPlan {
        if matches!((reader, stream), (ReaderKind::Byob, ReadableKind::Default)) {
            AcquireReaderPlan::RejectIncompatibleByob
        } else if self.locked {
            AcquireReaderPlan::RejectLocked
        } else {
            AcquireReaderPlan::Acquire(self.locked_transition(true))
        }
    }

    #[must_use]
    pub const fn plan_cancel_entry(self) -> CancelEntryPlan {
        if self.locked {
            CancelEntryPlan::RejectLocked
        } else {
            CancelEntryPlan::Continue
        }
    }

    #[must_use]
    pub const fn plan_disturb(self) -> ReadableAccessTransition {
        ReadableAccessTransition {
            source: self,
            next: Self {
                locked: self.locked,
                disturbed: true,
            },
        }
    }

    #[must_use]
    pub const fn plan_unlock(self) -> ReadableAccessTransition {
        self.locked_transition(false)
    }

    const fn locked_transition(self, locked: bool) -> ReadableAccessTransition {
        ReadableAccessTransition {
            source: self,
            next: Self {
                locked,
                disturbed: self.disturbed,
            },
        }
    }
}

/// An immediate access-state transition. The adapter must apply it without
/// running author JavaScript after reading `source`; a longer-lived operation
/// must discard it and plan again from a fresh snapshot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReadableAccessTransition {
    source: ReadableAccessSnapshot,
    next: ReadableAccessSnapshot,
}

impl ReadableAccessTransition {
    #[must_use]
    pub const fn source(self) -> ReadableAccessSnapshot {
        self.source
    }

    #[must_use]
    pub const fn next(self) -> ReadableAccessSnapshot {
        self.next
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AcquireReaderPlan {
    Acquire(ReadableAccessTransition),
    RejectLocked,
    RejectIncompatibleByob,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReadableLockPlan {
    Lock(ReadableAccessTransition),
    RejectLocked,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CancelEntryPlan {
    Continue,
    RejectLocked,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReaderReleaseSnapshot {
    attached: bool,
    access: ReadableAccessSnapshot,
    stream_kind: ReadableKind,
    stream_closed: bool,
    has_pending_closed_request: bool,
}

impl ReaderReleaseSnapshot {
    #[must_use]
    pub const fn detached() -> Self {
        Self {
            attached: false,
            access: ReadableAccessSnapshot::new(false, false),
            stream_kind: ReadableKind::Default,
            stream_closed: false,
            has_pending_closed_request: false,
        }
    }

    #[must_use]
    pub const fn attached(
        access: ReadableAccessSnapshot,
        stream_kind: ReadableKind,
        stream_closed: bool,
        has_pending_closed_request: bool,
    ) -> Self {
        Self {
            attached: true,
            access,
            stream_kind,
            stream_closed,
            has_pending_closed_request,
        }
    }

    #[must_use]
    pub const fn plan(self) -> ReleaseReaderPlan {
        if !self.attached {
            return ReleaseReaderPlan::AlreadyReleased;
        }
        let closed_promise = if self.stream_closed {
            ReleasedReaderClosedPromisePlan::ReplaceWithRejected
        } else if self.has_pending_closed_request {
            ReleasedReaderClosedPromisePlan::RejectExisting
        } else {
            ReleasedReaderClosedPromisePlan::CreateRejected
        };
        ReleaseReaderPlan::Release {
            access: self.access.plan_unlock(),
            release_byte_controller: matches!(self.stream_kind, ReadableKind::Byte),
            closed_promise,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReleaseReaderPlan {
    AlreadyReleased,
    Release {
        access: ReadableAccessTransition,
        release_byte_controller: bool,
        closed_promise: ReleasedReaderClosedPromisePlan,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReleasedReaderClosedPromisePlan {
    RejectExisting,
    ReplaceWithRejected,
    CreateRejected,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(
        state: ReadableState,
        close_requested: bool,
        queue_empty: bool,
    ) -> ReadableSnapshot {
        ReadableSnapshot::new(state, close_requested, queue_empty, 2)
    }

    #[test]
    fn storage_state_prefers_the_stored_error() {
        assert_eq!(
            ReadableState::from_storage(false, false),
            ReadableState::Readable
        );
        assert_eq!(
            ReadableState::from_storage(true, false),
            ReadableState::Closed
        );
        assert_eq!(
            ReadableState::from_storage(false, true),
            ReadableState::Errored
        );
        assert_eq!(
            ReadableState::from_storage(true, true),
            ReadableState::Errored
        );
    }

    #[test]
    fn enqueue_and_close_requests_only_admit_a_live_open_stream() {
        for state in [
            ReadableState::Readable,
            ReadableState::Closed,
            ReadableState::Errored,
        ] {
            for close_requested in [false, true] {
                let current = snapshot(state, close_requested, false);
                let admitted = state == ReadableState::Readable && !close_requested;
                assert_eq!(
                    current.plan_enqueue(),
                    if admitted {
                        EnqueuePlan::Continue
                    } else {
                        EnqueuePlan::Reject
                    }
                );
                assert_eq!(
                    current.plan_request_close(),
                    if admitted {
                        CloseRequestPlan::Request
                    } else {
                        CloseRequestPlan::Ignore
                    }
                );
            }
        }
    }

    #[test]
    fn finish_close_requires_a_requested_empty_readable_stream() {
        for state in [
            ReadableState::Readable,
            ReadableState::Closed,
            ReadableState::Errored,
        ] {
            for close_requested in [false, true] {
                for queue_empty in [false, true] {
                    let should_finish =
                        state == ReadableState::Readable && close_requested && queue_empty;
                    assert_eq!(
                        snapshot(state, close_requested, queue_empty).plan_finish_close(),
                        if should_finish {
                            FinishClosePlan::Finish
                        } else {
                            FinishClosePlan::Wait
                        }
                    );
                }
            }
        }
    }

    #[test]
    fn read_closed_promise_error_and_cancel_partition_lifecycle_states() {
        let readable = snapshot(ReadableState::Readable, true, true);
        assert_eq!(readable.plan_read_start(), ReadStartPlan::Continue);
        assert_eq!(readable.plan_closed_promise(), ClosedPromisePlan::Wait);
        assert_eq!(readable.plan_error(), ErrorPlan::Error);
        assert_eq!(
            readable.plan_cancel(),
            CancelPlan::RunAlgorithm {
                finish_requested_close: true,
            }
        );

        let closed = snapshot(ReadableState::Closed, false, true);
        assert_eq!(closed.plan_read_start(), ReadStartPlan::ResolveDone);
        assert_eq!(closed.plan_closed_promise(), ClosedPromisePlan::Resolve);
        assert_eq!(closed.plan_error(), ErrorPlan::Ignore);
        assert_eq!(closed.plan_cancel(), CancelPlan::Resolve);

        let errored = snapshot(ReadableState::Errored, false, true);
        assert_eq!(errored.plan_read_start(), ReadStartPlan::RejectStoredError);
        assert_eq!(
            errored.plan_closed_promise(),
            ClosedPromisePlan::RejectStoredError
        );
        assert_eq!(errored.plan_error(), ErrorPlan::Ignore);
        assert_eq!(errored.plan_cancel(), CancelPlan::RejectStoredError);
    }

    #[test]
    fn reader_acquisition_preserves_entrypoint_validation_order() {
        let unlocked = ReadableAccessSnapshot::new(false, false);
        let locked = ReadableAccessSnapshot::new(true, false);

        assert!(matches!(unlocked.plan_lock(), ReadableLockPlan::Lock(_)));
        assert_eq!(locked.plan_lock(), ReadableLockPlan::RejectLocked);

        assert_eq!(
            unlocked.plan_get_reader(ReaderKind::Byob, ReadableKind::Default),
            AcquireReaderPlan::RejectIncompatibleByob
        );
        assert_eq!(
            locked.plan_get_reader(ReaderKind::Byob, ReadableKind::Default),
            AcquireReaderPlan::RejectLocked
        );
        assert_eq!(
            locked.plan_reader_constructor(ReaderKind::Byob, ReadableKind::Default),
            AcquireReaderPlan::RejectIncompatibleByob
        );

        let AcquireReaderPlan::Acquire(acquire) =
            unlocked.plan_get_reader(ReaderKind::Default, ReadableKind::Default)
        else {
            panic!("an unlocked default stream should admit a default reader")
        };
        assert_eq!(acquire.source(), unlocked);
        assert!(acquire.next().locked());
        assert!(!acquire.next().disturbed());
    }

    #[test]
    fn cancel_disturb_and_unlock_are_explicit_access_plans() {
        let unlocked = ReadableAccessSnapshot::new(false, false);
        assert_eq!(unlocked.plan_cancel_entry(), CancelEntryPlan::Continue);

        let disturbed = unlocked.plan_disturb();
        assert_eq!(disturbed.source(), unlocked);
        assert!(disturbed.next().disturbed());
        assert!(!disturbed.next().locked());

        let locked = ReadableAccessSnapshot::new(true, true);
        assert_eq!(locked.plan_cancel_entry(), CancelEntryPlan::RejectLocked);
        let unlocked = locked.plan_unlock();
        assert_eq!(unlocked.source(), locked);
        assert!(!unlocked.next().locked());
        assert!(unlocked.next().disturbed());
    }

    #[test]
    fn reader_release_partitions_attachment_kind_and_closed_promise_effects() {
        assert_eq!(
            ReaderReleaseSnapshot::detached().plan(),
            ReleaseReaderPlan::AlreadyReleased
        );

        for (stream_closed, has_pending, expected) in [
            (false, true, ReleasedReaderClosedPromisePlan::RejectExisting),
            (
                false,
                false,
                ReleasedReaderClosedPromisePlan::CreateRejected,
            ),
            (
                true,
                true,
                ReleasedReaderClosedPromisePlan::ReplaceWithRejected,
            ),
            (
                true,
                false,
                ReleasedReaderClosedPromisePlan::ReplaceWithRejected,
            ),
        ] {
            let plan = ReaderReleaseSnapshot::attached(
                ReadableAccessSnapshot::new(true, true),
                ReadableKind::Byte,
                stream_closed,
                has_pending,
            )
            .plan();
            let ReleaseReaderPlan::Release {
                access,
                release_byte_controller,
                closed_promise,
            } = plan
            else {
                panic!("an attached reader should release")
            };
            assert!(release_byte_controller);
            assert_eq!(closed_promise, expected);
            assert!(!access.next().locked());
            assert!(access.next().disturbed());
        }
    }
}
