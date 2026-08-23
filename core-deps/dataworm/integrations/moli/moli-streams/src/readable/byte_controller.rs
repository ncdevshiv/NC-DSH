//! Runtime-independent `ReadableByteStreamController` state and pull-into
//! descriptor transitions.

use super::{EnqueuePlan, ReadStartPlan, ReadableSnapshot, ReadableState};

/// The reader that owns a pull-into descriptor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum PullIntoReaderType {
    Default = 0,
    Byob = 1,
    None = 2,
}

impl PullIntoReaderType {
    #[must_use]
    pub const fn from_discriminant(value: u32) -> Option<Self> {
        match value {
            0 => Some(Self::Default),
            1 => Some(Self::Byob),
            2 => Some(Self::None),
            _ => None,
        }
    }
}

/// The consumer view brand which must be reconstructed when a BYOB read
/// settles.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum ArrayBufferViewKind {
    DataView = 0,
    Int8 = 1,
    Uint8 = 2,
    Uint8Clamped = 3,
    Int16 = 4,
    Uint16 = 5,
    Int32 = 6,
    Uint32 = 7,
    Float16 = 8,
    Float32 = 9,
    Float64 = 10,
    BigInt64 = 11,
    BigUint64 = 12,
}

impl ArrayBufferViewKind {
    #[must_use]
    pub const fn from_discriminant(value: u32) -> Option<Self> {
        match value {
            0 => Some(Self::DataView),
            1 => Some(Self::Int8),
            2 => Some(Self::Uint8),
            3 => Some(Self::Uint8Clamped),
            4 => Some(Self::Int16),
            5 => Some(Self::Uint16),
            6 => Some(Self::Int32),
            7 => Some(Self::Uint32),
            8 => Some(Self::Float16),
            9 => Some(Self::Float32),
            10 => Some(Self::Float64),
            11 => Some(Self::BigInt64),
            12 => Some(Self::BigUint64),
            _ => None,
        }
    }

    #[must_use]
    pub const fn element_size(self) -> usize {
        match self {
            Self::DataView | Self::Int8 | Self::Uint8 | Self::Uint8Clamped => 1,
            Self::Int16 | Self::Uint16 | Self::Float16 => 2,
            Self::Int32 | Self::Uint32 | Self::Float32 => 4,
            Self::Float64 | Self::BigInt64 | Self::BigUint64 => 8,
        }
    }
}

/// An absolute byte range within the descriptor backing buffer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ByteRange {
    offset: usize,
    length: usize,
}

impl ByteRange {
    const fn new(offset: usize, length: usize) -> Self {
        Self { offset, length }
    }

    #[must_use]
    pub const fn offset(self) -> usize {
        self.offset
    }

    #[must_use]
    pub const fn length(self) -> usize {
        self.length
    }

    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.length == 0
    }

    #[must_use]
    pub const fn end(self) -> usize {
        self.offset + self.length
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DescriptorError {
    EmptyView,
    InvalidBounds,
    InvalidMinimumFill,
    InvalidElementAlignment,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReadIntoError {
    EmptyView,
    ZeroMinimum,
    MinimumExceedsCapacity,
    InvalidBounds,
}

/// A runtime-independent snapshot of a pull-into descriptor.
///
/// It deliberately contains no buffer handle. A runtime adapter creates this
/// value from its traced storage and must treat it as invalid after any call
/// that can execute author JavaScript or replace the live queue generation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PullIntoDescriptor {
    buffer_byte_length: usize,
    byte_offset: usize,
    byte_length: usize,
    bytes_filled: usize,
    minimum_fill: usize,
    view_kind: ArrayBufferViewKind,
    reader_type: PullIntoReaderType,
}

impl PullIntoDescriptor {
    pub fn new(
        buffer_byte_length: usize,
        byte_offset: usize,
        byte_length: usize,
        bytes_filled: usize,
        minimum_fill: usize,
        view_kind: ArrayBufferViewKind,
        reader_type: PullIntoReaderType,
    ) -> Result<Self, DescriptorError> {
        let element_size = view_kind.element_size();
        if byte_length == 0 {
            return Err(DescriptorError::EmptyView);
        }
        if minimum_fill == 0 || minimum_fill > byte_length {
            return Err(DescriptorError::InvalidMinimumFill);
        }
        if !byte_offset.is_multiple_of(element_size)
            || !byte_length.is_multiple_of(element_size)
            || !minimum_fill.is_multiple_of(element_size)
            || (bytes_filled >= minimum_fill && !bytes_filled.is_multiple_of(element_size))
        {
            return Err(DescriptorError::InvalidElementAlignment);
        }
        if bytes_filled > byte_length
            || byte_offset
                .checked_add(byte_length)
                .is_none_or(|end| end > buffer_byte_length)
        {
            return Err(DescriptorError::InvalidBounds);
        }
        Ok(Self {
            buffer_byte_length,
            byte_offset,
            byte_length,
            bytes_filled,
            minimum_fill,
            view_kind,
            reader_type,
        })
    }

    pub fn for_read(
        buffer_byte_length: usize,
        byte_offset: usize,
        byte_length: usize,
        minimum_elements: usize,
        view_kind: ArrayBufferViewKind,
        reader_type: PullIntoReaderType,
    ) -> Result<Self, ReadIntoError> {
        if buffer_byte_length == 0 || byte_length == 0 {
            return Err(ReadIntoError::EmptyView);
        }
        if minimum_elements == 0 {
            return Err(ReadIntoError::ZeroMinimum);
        }
        let element_size = view_kind.element_size();
        let capacity_elements = byte_length / element_size;
        if minimum_elements > capacity_elements {
            return Err(ReadIntoError::MinimumExceedsCapacity);
        }
        let Some(minimum_fill) = minimum_elements.checked_mul(element_size) else {
            return Err(ReadIntoError::MinimumExceedsCapacity);
        };
        Self::new(
            buffer_byte_length,
            byte_offset,
            byte_length,
            0,
            minimum_fill,
            view_kind,
            reader_type,
        )
        .map_err(|_| ReadIntoError::InvalidBounds)
    }

    #[must_use]
    pub const fn buffer_byte_length(self) -> usize {
        self.buffer_byte_length
    }

    #[must_use]
    pub const fn byte_offset(self) -> usize {
        self.byte_offset
    }

    #[must_use]
    pub const fn byte_length(self) -> usize {
        self.byte_length
    }

    #[must_use]
    pub const fn bytes_filled(self) -> usize {
        self.bytes_filled
    }

    #[must_use]
    pub const fn minimum_fill(self) -> usize {
        self.minimum_fill
    }

    #[must_use]
    pub const fn element_size(self) -> usize {
        self.view_kind.element_size()
    }

    #[must_use]
    pub const fn view_kind(self) -> ArrayBufferViewKind {
        self.view_kind
    }

    #[must_use]
    pub const fn reader_type(self) -> PullIntoReaderType {
        self.reader_type
    }

    #[must_use]
    pub const fn remaining(self) -> usize {
        self.byte_length - self.bytes_filled
    }

    #[must_use]
    pub const fn request_range(self) -> ByteRange {
        ByteRange::new(
            self.byte_offset + self.bytes_filled,
            self.byte_length - self.bytes_filled,
        )
    }

    #[must_use]
    pub const fn filled_range(self) -> ByteRange {
        ByteRange::new(self.byte_offset, self.bytes_filled)
    }

    #[must_use]
    pub const fn released(mut self) -> Self {
        self.reader_type = PullIntoReaderType::None;
        self
    }

    #[must_use]
    pub fn plan_fill(self, available_bytes: usize) -> FillPlan {
        let remaining = self.remaining();
        if remaining == 0 {
            return FillPlan::new(0, self, true);
        }
        let maximum = available_bytes.min(remaining);
        if maximum == 0 {
            return FillPlan::new(0, self, self.is_ready());
        }
        let candidate = self.bytes_filled + maximum;
        let amount = if candidate >= self.minimum_fill {
            let aligned_total = candidate / self.element_size() * self.element_size();
            aligned_total - self.bytes_filled
        } else {
            maximum
        };
        if amount == 0 {
            return FillPlan::new(0, self, self.is_ready());
        }
        let mut next = self;
        next.bytes_filled += amount;
        let ready = next.bytes_filled >= next.minimum_fill
            && next.bytes_filled.is_multiple_of(next.element_size());
        FillPlan::new(amount, next, ready)
    }

    const fn is_ready(self) -> bool {
        self.bytes_filled >= self.minimum_fill
            && self.bytes_filled.is_multiple_of(self.element_size())
    }

    pub fn plan_respond(
        self,
        stream_state: ReadableByteStreamState,
        bytes_written: usize,
    ) -> Result<RespondPlan, RespondError> {
        if bytes_written > self.remaining() {
            return Err(RespondError::ExceedsRemaining);
        }
        match stream_state {
            ReadableByteStreamState::Readable if bytes_written == 0 => {
                return Err(RespondError::ZeroWhileReadable);
            }
            ReadableByteStreamState::Closed if bytes_written != 0 => {
                return Err(RespondError::NonZeroWhileClosed);
            }
            _ => {}
        }
        Ok(self.finish_respond(stream_state, bytes_written))
    }

    pub fn plan_respond_with_new_view(
        self,
        stream_state: ReadableByteStreamState,
        view: ReplacementView,
    ) -> Result<RespondPlan, ReplacementViewError> {
        if view.detached {
            return Err(ReplacementViewError::Detached);
        }
        if stream_state == ReadableByteStreamState::Readable && view.byte_length == 0 {
            return Err(ReplacementViewError::EmptyWhileReadable);
        }
        if view.buffer_byte_length != self.buffer_byte_length {
            return Err(ReplacementViewError::BufferLengthMismatch);
        }
        if view.byte_offset != self.request_range().offset || view.byte_length > self.remaining() {
            return Err(ReplacementViewError::InvalidBounds);
        }
        if stream_state == ReadableByteStreamState::Closed && view.byte_length != 0 {
            return Err(ReplacementViewError::NonEmptyWhileClosed);
        }
        Ok(self.finish_respond(stream_state, view.byte_length))
    }

    pub fn validate_close(self) -> Result<(), CloseError> {
        if self.bytes_filled.is_multiple_of(self.element_size()) {
            Ok(())
        } else {
            Err(CloseError::PartiallyFilledElement)
        }
    }

    #[must_use]
    pub const fn plan_resolution(self, done: bool) -> DescriptorResolutionPlan {
        let result = if done && matches!(self.reader_type, PullIntoReaderType::Default) {
            DescriptorResultPlan::Undefined
        } else {
            DescriptorResultPlan::View {
                kind: self.view_kind,
                range: ByteRange::new(self.byte_offset, self.bytes_filled),
            }
        };
        DescriptorResolutionPlan {
            source: self,
            done,
            result,
        }
    }

    fn finish_respond(
        self,
        stream_state: ReadableByteStreamState,
        bytes_written: usize,
    ) -> RespondPlan {
        let source = self;
        let mut next = self;
        next.bytes_filled += bytes_written;
        let action = if stream_state == ReadableByteStreamState::Closed {
            RespondAction::Closed
        } else if next.reader_type == PullIntoReaderType::None {
            RespondAction::FlushReleased
        } else if next.bytes_filled < next.minimum_fill {
            RespondAction::AwaitMore
        } else {
            let remainder_length = next.bytes_filled % next.element_size();
            let aligned = next.bytes_filled - remainder_length;
            let remainder = ByteRange::new(next.byte_offset + aligned, remainder_length);
            next.bytes_filled = aligned;
            RespondAction::Commit { remainder }
        };
        RespondPlan {
            source,
            descriptor: next,
            action,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FillPlan {
    bytes_to_copy: usize,
    destination: ByteRange,
    descriptor: PullIntoDescriptor,
    ready: bool,
}

impl FillPlan {
    fn new(bytes_to_copy: usize, descriptor: PullIntoDescriptor, ready: bool) -> Self {
        let destination = ByteRange::new(
            descriptor.byte_offset + descriptor.bytes_filled - bytes_to_copy,
            bytes_to_copy,
        );
        Self {
            bytes_to_copy,
            destination,
            descriptor,
            ready,
        }
    }

    #[must_use]
    pub const fn bytes_to_copy(self) -> usize {
        self.bytes_to_copy
    }

    #[must_use]
    pub const fn destination(self) -> ByteRange {
        self.destination
    }

    #[must_use]
    pub const fn descriptor(self) -> PullIntoDescriptor {
        self.descriptor
    }

    #[must_use]
    pub const fn ready(self) -> bool {
        self.ready
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReadableByteStreamState {
    Readable,
    Closed,
}

/// The live descriptor and lifecycle state used by the two BYOB response
/// entrypoints.
///
/// The adapter must still validate that `source` identifies its current head
/// descriptor before committing buffer or queue mutations.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ByteRespondContext {
    source: PullIntoDescriptor,
    stream_state: ReadableByteStreamState,
}

impl ByteRespondContext {
    #[must_use]
    pub const fn source(self) -> PullIntoDescriptor {
        self.source
    }

    pub fn plan_respond(self, bytes_written: usize) -> Result<RespondPlan, RespondError> {
        self.source.plan_respond(self.stream_state, bytes_written)
    }

    pub fn plan_respond_with_new_view(
        self,
        view: ReplacementView,
    ) -> Result<RespondPlan, ReplacementViewError> {
        self.source
            .plan_respond_with_new_view(self.stream_state, view)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RespondError {
    ExceedsRemaining,
    ZeroWhileReadable,
    NonZeroWhileClosed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RespondAction {
    AwaitMore,
    Commit { remainder: ByteRange },
    FlushReleased,
    Closed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RespondPlan {
    source: PullIntoDescriptor,
    descriptor: PullIntoDescriptor,
    action: RespondAction,
}

impl RespondPlan {
    #[must_use]
    pub const fn source(self) -> PullIntoDescriptor {
        self.source
    }

    #[must_use]
    pub const fn descriptor(self) -> PullIntoDescriptor {
        self.descriptor
    }

    #[must_use]
    pub const fn action(self) -> RespondAction {
        self.action
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReplacementView {
    pub buffer_byte_length: usize,
    pub byte_offset: usize,
    pub byte_length: usize,
    pub detached: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReplacementViewError {
    Detached,
    EmptyWhileReadable,
    BufferLengthMismatch,
    InvalidBounds,
    NonEmptyWhileClosed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CloseError {
    PartiallyFilledElement,
}

/// A point-in-time byte-controller snapshot decoded from adapter-owned queue,
/// descriptor, and request storage.
///
/// Invalid adapter descriptors remain as `None` entries so an invalid head is
/// not accidentally skipped in favor of a later valid descriptor. Runtime
/// handles, buffers, and request wrapper identity stay in the adapter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ByteControllerSnapshot {
    readable: ReadableSnapshot,
    descriptors: Vec<Option<PullIntoDescriptor>>,
    cached_byob_request: bool,
    auto_allocate_chunk_size: Option<usize>,
    pipe_read_pending: Option<bool>,
}

impl ByteControllerSnapshot {
    #[must_use]
    pub fn new(
        readable: ReadableSnapshot,
        descriptors: Vec<Option<PullIntoDescriptor>>,
        cached_byob_request: bool,
        auto_allocate_chunk_size: Option<usize>,
        pipe_read_pending: Option<bool>,
    ) -> Self {
        Self {
            readable,
            descriptors,
            cached_byob_request,
            auto_allocate_chunk_size,
            pipe_read_pending,
        }
    }

    #[must_use]
    pub const fn readable(&self) -> ReadableSnapshot {
        self.readable
    }

    #[must_use]
    pub fn descriptors(&self) -> &[Option<PullIntoDescriptor>] {
        &self.descriptors
    }

    #[must_use]
    pub fn head_descriptor(&self) -> Option<PullIntoDescriptor> {
        self.descriptors.first().copied().flatten()
    }

    #[must_use]
    pub fn plan_byob_request(&self) -> ByobRequestPlan {
        if self.cached_byob_request {
            return ByobRequestPlan::UseCached;
        }
        self.head_descriptor()
            .map_or(ByobRequestPlan::Unavailable, |source| {
                ByobRequestPlan::Create(ByobRequestCreationPlan {
                    source,
                    view: source.request_range(),
                })
            })
    }

    #[must_use]
    pub fn plan_pending_byob_view(&self) -> ByobRequestPlan {
        if self
            .head_descriptor()
            .is_none_or(|descriptor| descriptor.reader_type() != PullIntoReaderType::Byob)
        {
            return ByobRequestPlan::Unavailable;
        }
        self.plan_byob_request()
    }

    /// Joins the current descriptor head with the stream lifecycle for
    /// `respond()` and `respondWithNewView()` validation.
    ///
    /// An errored stream cannot retain a usable BYOB request through the
    /// public algorithms. Treating an impossible retained descriptor as
    /// readable preserves the previous adapter behavior while leaving error
    /// cleanup ownership with the readable-stream lifecycle.
    #[must_use]
    pub fn respond_context(&self) -> Option<ByteRespondContext> {
        let source = self.head_descriptor()?;
        let stream_state = match self.readable.state() {
            ReadableState::Closed => ReadableByteStreamState::Closed,
            ReadableState::Readable | ReadableState::Errored => ReadableByteStreamState::Readable,
        };
        Some(ByteRespondContext {
            source,
            stream_state,
        })
    }

    #[must_use]
    pub fn plan_auto_allocate(&self) -> Option<AutoAllocatePlan> {
        let byte_length = self.auto_allocate_chunk_size.filter(|size| *size != 0)?;
        let descriptor = PullIntoDescriptor::new(
            byte_length,
            0,
            byte_length,
            0,
            1,
            ArrayBufferViewKind::Uint8,
            PullIntoReaderType::Default,
        )
        .expect("a non-empty Uint8 auto-allocation descriptor is always valid");
        Some(AutoAllocatePlan { descriptor })
    }

    #[must_use]
    pub fn plan_enqueue_start(&self) -> ByteEnqueueStartPlan {
        if self.readable.plan_enqueue() == EnqueuePlan::Reject {
            return ByteEnqueueStartPlan::Reject;
        }
        self.pipe_read_pending
            .map_or(ByteEnqueueStartPlan::Continue, |pending| {
                ByteEnqueueStartPlan::Pipe(BytePipeEnqueuePlan {
                    fulfills_pending_read: pending,
                })
            })
    }

    #[must_use]
    pub fn plan_enqueue_after_released(&self) -> ByteEnqueueContinuationPlan {
        match self.head_descriptor() {
            Some(source) if source.reader_type() == PullIntoReaderType::Default => {
                ByteEnqueueContinuationPlan::FulfillDefaultDescriptor { source }
            }
            Some(source) if source.reader_type() == PullIntoReaderType::Byob => {
                ByteEnqueueContinuationPlan::QueueWithByobDescriptor { source }
            }
            _ => ByteEnqueueContinuationPlan::Queue,
        }
    }

    #[must_use]
    pub fn plan_read_into(&self) -> ByteReadIntoPlan {
        match self.readable.plan_read_start() {
            ReadStartPlan::RejectStoredError => ByteReadIntoPlan::RejectStoredError,
            ReadStartPlan::ResolveDone => ByteReadIntoPlan::ResolveDone,
            ReadStartPlan::Continue => self
                .head_descriptor()
                .map_or(ByteReadIntoPlan::FillFromQueue, |source| {
                    ByteReadIntoPlan::AppendBehindHead { source }
                }),
        }
    }

    pub fn plan_take_head_resolution(
        &self,
        done: bool,
    ) -> Result<DescriptorResolutionPlan, HeadResolutionError> {
        let descriptor = self
            .head_descriptor()
            .ok_or(HeadResolutionError::MissingDescriptor)?;
        if descriptor.reader_type() == PullIntoReaderType::None {
            return Err(HeadResolutionError::ReleasedDescriptor);
        }
        if !descriptor
            .bytes_filled()
            .is_multiple_of(descriptor.element_size())
        {
            return Err(HeadResolutionError::UnalignedDescriptor);
        }
        Ok(descriptor.plan_resolution(done))
    }

    #[must_use]
    pub fn plan_default_read_step(&self) -> DefaultReadStepPlan {
        if self.head_descriptor().is_some()
            || self.readable.pending_read_count() == 0
            || self.readable.queue_empty()
        {
            DefaultReadStepPlan::Wait
        } else {
            DefaultReadStepPlan::Dequeue
        }
    }

    #[must_use]
    pub fn plan_flush_released_head(&self) -> FlushReleasedHeadPlan {
        self.head_descriptor()
            .map_or(FlushReleasedHeadPlan::Done, |source| {
                if source.reader_type() == PullIntoReaderType::None {
                    FlushReleasedHeadPlan::Flush {
                        source,
                        bytes: source.filled_range(),
                    }
                } else {
                    FlushReleasedHeadPlan::Done
                }
            })
    }

    #[must_use]
    pub fn plan_closed_released_head(&self) -> ClosedReleasedHeadPlan {
        self.head_descriptor()
            .map_or(ClosedReleasedHeadPlan::Continue, |source| {
                if source.reader_type() == PullIntoReaderType::None {
                    ClosedReleasedHeadPlan::Remove { source }
                } else {
                    ClosedReleasedHeadPlan::Continue
                }
            })
    }

    pub fn plan_closed_byob_step(
        &self,
        remaining_pending_reads: usize,
    ) -> Result<ClosedByobStepPlan, HeadResolutionError> {
        if remaining_pending_reads == 0 {
            return Ok(ClosedByobStepPlan::Done);
        }
        let Some(head) = self.head_descriptor() else {
            return Ok(ClosedByobStepPlan::Done);
        };
        if head.reader_type() != PullIntoReaderType::Byob {
            return Ok(ClosedByobStepPlan::Done);
        }
        self.plan_take_head_resolution(true)
            .map(ClosedByobStepPlan::Resolve)
    }

    #[must_use]
    pub fn plan_release_reader(&self) -> ReleaseReaderPlan {
        self.head_descriptor()
            .map_or(ReleaseReaderPlan::None, |source| {
                ReleaseReaderPlan::RetainReleasedHead {
                    source,
                    descriptor: source.released(),
                }
            })
    }

    pub fn validate_close(&self) -> Result<(), CloseError> {
        if !self.readable.queue_empty() {
            return Ok(());
        }
        self.head_descriptor()
            .map_or(Ok(()), PullIntoDescriptor::validate_close)
    }

    #[must_use]
    pub fn plan_finish_close(&self) -> FinishByteClosePlan {
        if self
            .descriptors
            .iter()
            .flatten()
            .any(|descriptor| descriptor.reader_type() == PullIntoReaderType::Byob)
        {
            FinishByteClosePlan::WaitForByobResponse
        } else {
            FinishByteClosePlan::FinishDefaultReads
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ByobRequestPlan {
    Unavailable,
    UseCached,
    Create(ByobRequestCreationPlan),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ByobRequestCreationPlan {
    source: PullIntoDescriptor,
    view: ByteRange,
}

impl ByobRequestCreationPlan {
    #[must_use]
    pub const fn source(self) -> PullIntoDescriptor {
        self.source
    }

    #[must_use]
    pub const fn view(self) -> ByteRange {
        self.view
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AutoAllocatePlan {
    descriptor: PullIntoDescriptor,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ByteReadIntoPlan {
    RejectStoredError,
    ResolveDone,
    AppendBehindHead { source: PullIntoDescriptor },
    FillFromQueue,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DescriptorResolutionPlan {
    source: PullIntoDescriptor,
    done: bool,
    result: DescriptorResultPlan,
}

impl DescriptorResolutionPlan {
    #[must_use]
    pub const fn source(self) -> PullIntoDescriptor {
        self.source
    }

    #[must_use]
    pub const fn done(self) -> bool {
        self.done
    }

    #[must_use]
    pub const fn result(self) -> DescriptorResultPlan {
        self.result
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DescriptorResultPlan {
    Undefined,
    View {
        kind: ArrayBufferViewKind,
        range: ByteRange,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HeadResolutionError {
    MissingDescriptor,
    ReleasedDescriptor,
    UnalignedDescriptor,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DefaultReadStepPlan {
    Wait,
    Dequeue,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FlushReleasedHeadPlan {
    Done,
    Flush {
        source: PullIntoDescriptor,
        bytes: ByteRange,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClosedReleasedHeadPlan {
    Continue,
    Remove { source: PullIntoDescriptor },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClosedByobStepPlan {
    Done,
    Resolve(DescriptorResolutionPlan),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ByteEnqueueStartPlan {
    Reject,
    Pipe(BytePipeEnqueuePlan),
    Continue,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BytePipeEnqueuePlan {
    fulfills_pending_read: bool,
}

impl BytePipeEnqueuePlan {
    #[must_use]
    pub const fn fulfills_pending_read(self) -> bool {
        self.fulfills_pending_read
    }

    #[must_use]
    pub const fn queue_size(self, byte_length: usize) -> usize {
        if self.fulfills_pending_read {
            0
        } else {
            byte_length
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ByteEnqueueContinuationPlan {
    FulfillDefaultDescriptor { source: PullIntoDescriptor },
    QueueWithByobDescriptor { source: PullIntoDescriptor },
    Queue,
}

impl AutoAllocatePlan {
    #[must_use]
    pub const fn descriptor(self) -> PullIntoDescriptor {
        self.descriptor
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReleaseReaderPlan {
    None,
    RetainReleasedHead {
        source: PullIntoDescriptor,
        descriptor: PullIntoDescriptor,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FinishByteClosePlan {
    FinishDefaultReads,
    WaitForByobResponse,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn descriptor(
        bytes_filled: usize,
        minimum_fill: usize,
        view_kind: ArrayBufferViewKind,
        reader_type: PullIntoReaderType,
    ) -> PullIntoDescriptor {
        PullIntoDescriptor::new(
            32,
            8,
            16,
            bytes_filled,
            minimum_fill,
            view_kind,
            reader_type,
        )
        .unwrap()
    }

    #[test]
    fn view_kinds_round_trip_discriminants_and_element_sizes() {
        let expected = [
            (ArrayBufferViewKind::DataView, 1),
            (ArrayBufferViewKind::Int8, 1),
            (ArrayBufferViewKind::Uint8, 1),
            (ArrayBufferViewKind::Uint8Clamped, 1),
            (ArrayBufferViewKind::Int16, 2),
            (ArrayBufferViewKind::Uint16, 2),
            (ArrayBufferViewKind::Int32, 4),
            (ArrayBufferViewKind::Uint32, 4),
            (ArrayBufferViewKind::Float16, 2),
            (ArrayBufferViewKind::Float32, 4),
            (ArrayBufferViewKind::Float64, 8),
            (ArrayBufferViewKind::BigInt64, 8),
            (ArrayBufferViewKind::BigUint64, 8),
        ];
        for (index, (kind, size)) in expected.into_iter().enumerate() {
            assert_eq!(
                ArrayBufferViewKind::from_discriminant(index as u32),
                Some(kind)
            );
            assert_eq!(kind.element_size(), size);
        }
        assert_eq!(ArrayBufferViewKind::from_discriminant(13), None);
        assert_eq!(
            PullIntoReaderType::from_discriminant(0),
            Some(PullIntoReaderType::Default)
        );
        assert_eq!(
            PullIntoReaderType::from_discriminant(1),
            Some(PullIntoReaderType::Byob)
        );
        assert_eq!(
            PullIntoReaderType::from_discriminant(2),
            Some(PullIntoReaderType::None)
        );
        assert_eq!(PullIntoReaderType::from_discriminant(3), None);
    }

    #[test]
    fn descriptor_construction_rejects_invalid_bounds_minimum_and_alignment() {
        assert_eq!(
            PullIntoDescriptor::new(
                8,
                0,
                0,
                0,
                1,
                ArrayBufferViewKind::Uint8,
                PullIntoReaderType::Byob,
            ),
            Err(DescriptorError::EmptyView)
        );
        assert_eq!(
            PullIntoDescriptor::new(
                8,
                4,
                8,
                0,
                1,
                ArrayBufferViewKind::Uint8,
                PullIntoReaderType::Byob,
            ),
            Err(DescriptorError::InvalidBounds)
        );
        assert_eq!(
            PullIntoDescriptor::new(
                8,
                0,
                8,
                0,
                0,
                ArrayBufferViewKind::Uint8,
                PullIntoReaderType::Byob,
            ),
            Err(DescriptorError::InvalidMinimumFill)
        );
        assert_eq!(
            PullIntoDescriptor::new(
                8,
                1,
                6,
                0,
                2,
                ArrayBufferViewKind::Uint16,
                PullIntoReaderType::Byob,
            ),
            Err(DescriptorError::InvalidElementAlignment)
        );
    }

    #[test]
    fn read_plan_uses_element_counts_and_preserves_validation_order() {
        assert_eq!(
            PullIntoDescriptor::for_read(
                0,
                0,
                0,
                0,
                ArrayBufferViewKind::Uint16,
                PullIntoReaderType::Byob,
            ),
            Err(ReadIntoError::EmptyView)
        );
        assert_eq!(
            PullIntoDescriptor::for_read(
                8,
                0,
                8,
                0,
                ArrayBufferViewKind::Uint16,
                PullIntoReaderType::Byob,
            ),
            Err(ReadIntoError::ZeroMinimum)
        );
        assert_eq!(
            PullIntoDescriptor::for_read(
                8,
                0,
                8,
                5,
                ArrayBufferViewKind::Uint16,
                PullIntoReaderType::Byob,
            ),
            Err(ReadIntoError::MinimumExceedsCapacity)
        );
        let state = PullIntoDescriptor::for_read(
            16,
            4,
            8,
            3,
            ArrayBufferViewKind::Uint16,
            PullIntoReaderType::Byob,
        )
        .unwrap();
        assert_eq!(state.minimum_fill(), 6);
        assert_eq!(state.request_range(), ByteRange::new(4, 8));
    }

    #[test]
    fn fill_plan_accumulates_partial_bytes_then_commits_an_aligned_prefix() {
        let initial = descriptor(0, 4, ArrayBufferViewKind::Uint16, PullIntoReaderType::Byob);
        let first = initial.plan_fill(1);
        assert_eq!(first.bytes_to_copy(), 1);
        assert_eq!(first.destination(), ByteRange::new(8, 1));
        assert_eq!(first.descriptor().bytes_filled(), 1);
        assert!(!first.ready());

        let second = first.descriptor().plan_fill(4);
        assert_eq!(second.bytes_to_copy(), 3);
        assert_eq!(second.destination(), ByteRange::new(9, 3));
        assert_eq!(second.descriptor().bytes_filled(), 4);
        assert!(second.ready());

        let capacity =
            descriptor(14, 4, ArrayBufferViewKind::Uint16, PullIntoReaderType::Byob).plan_fill(99);
        assert_eq!(capacity.bytes_to_copy(), 2);
        assert_eq!(capacity.descriptor().bytes_filled(), 16);
        assert!(capacity.ready());
    }

    #[test]
    fn fill_plan_exhaustively_preserves_descriptor_bounds_and_progress() {
        for kind in [
            ArrayBufferViewKind::Uint8,
            ArrayBufferViewKind::Uint16,
            ArrayBufferViewKind::Uint32,
            ArrayBufferViewKind::Float64,
        ] {
            let element_size = kind.element_size();
            for element_capacity in 1..=6 {
                let byte_length = element_capacity * element_size;
                for minimum_elements in 1..=element_capacity {
                    let minimum_fill = minimum_elements * element_size;
                    for bytes_filled in 0..=byte_length {
                        let state = PullIntoDescriptor::new(
                            byte_length + element_size * 2,
                            element_size,
                            byte_length,
                            bytes_filled,
                            minimum_fill,
                            kind,
                            PullIntoReaderType::Byob,
                        );
                        if bytes_filled >= minimum_fill
                            && !bytes_filled.is_multiple_of(element_size)
                        {
                            assert_eq!(state, Err(DescriptorError::InvalidElementAlignment));
                            continue;
                        }
                        let state = state.unwrap();
                        for available in 0..=byte_length + element_size {
                            let plan = state.plan_fill(available);
                            let next = plan.descriptor();
                            assert!(plan.bytes_to_copy() <= available);
                            assert!(plan.bytes_to_copy() <= state.remaining());
                            assert_eq!(
                                next.bytes_filled(),
                                state.bytes_filled() + plan.bytes_to_copy()
                            );
                            assert!(next.bytes_filled() <= next.byte_length());
                            assert_eq!(
                                plan.destination().offset(),
                                state.byte_offset() + state.bytes_filled()
                            );
                            assert_eq!(plan.destination().length(), plan.bytes_to_copy());
                            assert!(plan.destination().end() <= next.buffer_byte_length());
                            assert_eq!(
                                plan.ready(),
                                next.bytes_filled() >= next.minimum_fill()
                                    && next.bytes_filled().is_multiple_of(next.element_size())
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn respond_plan_distinguishes_await_commit_release_and_closed() {
        let initial = descriptor(0, 4, ArrayBufferViewKind::Uint16, PullIntoReaderType::Byob);
        assert_eq!(
            initial.plan_respond(ReadableByteStreamState::Readable, 0),
            Err(RespondError::ZeroWhileReadable)
        );
        assert_eq!(
            initial.plan_respond(ReadableByteStreamState::Closed, 1),
            Err(RespondError::NonZeroWhileClosed)
        );
        assert_eq!(
            initial.plan_respond(ReadableByteStreamState::Readable, 17),
            Err(RespondError::ExceedsRemaining)
        );

        let partial = initial
            .plan_respond(ReadableByteStreamState::Readable, 1)
            .unwrap();
        assert_eq!(partial.action(), RespondAction::AwaitMore);
        assert_eq!(partial.descriptor().bytes_filled(), 1);

        let committed = partial
            .descriptor()
            .plan_respond(ReadableByteStreamState::Readable, 4)
            .unwrap();
        assert_eq!(
            committed.action(),
            RespondAction::Commit {
                remainder: ByteRange::new(12, 1)
            }
        );
        assert_eq!(committed.descriptor().bytes_filled(), 4);

        let released = initial
            .released()
            .plan_respond(ReadableByteStreamState::Readable, 3)
            .unwrap();
        assert_eq!(released.action(), RespondAction::FlushReleased);
        assert_eq!(released.descriptor().bytes_filled(), 3);

        let closed = initial
            .plan_respond(ReadableByteStreamState::Closed, 0)
            .unwrap();
        assert_eq!(closed.action(), RespondAction::Closed);
    }

    #[test]
    fn respond_plan_exhaustively_partitions_written_bytes_without_loss() {
        for kind in [
            ArrayBufferViewKind::Uint8,
            ArrayBufferViewKind::Uint16,
            ArrayBufferViewKind::Uint32,
            ArrayBufferViewKind::Float64,
        ] {
            let element_size = kind.element_size();
            for minimum_elements in 1..=4 {
                let minimum_fill = minimum_elements * element_size;
                let byte_length = 4 * element_size;
                for bytes_filled in 0..minimum_fill {
                    let state = PullIntoDescriptor::new(
                        byte_length,
                        0,
                        byte_length,
                        bytes_filled,
                        minimum_fill,
                        kind,
                        PullIntoReaderType::Byob,
                    )
                    .unwrap();
                    for bytes_written in 1..=state.remaining() {
                        let plan = state
                            .plan_respond(ReadableByteStreamState::Readable, bytes_written)
                            .unwrap();
                        match plan.action() {
                            RespondAction::AwaitMore => {
                                assert_eq!(
                                    plan.descriptor().bytes_filled(),
                                    bytes_filled + bytes_written
                                );
                                assert!(plan.descriptor().bytes_filled() < minimum_fill);
                            }
                            RespondAction::Commit { remainder } => {
                                assert!(
                                    plan.descriptor()
                                        .bytes_filled()
                                        .is_multiple_of(element_size)
                                );
                                assert_eq!(
                                    plan.descriptor().bytes_filled() + remainder.length(),
                                    bytes_filled + bytes_written
                                );
                                assert_eq!(
                                    remainder.offset(),
                                    plan.descriptor().byte_offset()
                                        + plan.descriptor().bytes_filled()
                                );
                                assert!(remainder.length() < element_size);
                            }
                            RespondAction::FlushReleased | RespondAction::Closed => {
                                panic!("active readable descriptor chose a terminal action")
                            }
                        }
                    }
                }
            }
        }

        let state = descriptor(0, 2, ArrayBufferViewKind::Uint16, PullIntoReaderType::Byob);
        assert_eq!(
            state.plan_respond(ReadableByteStreamState::Closed, state.remaining() + 1),
            Err(RespondError::ExceedsRemaining)
        );
    }

    #[test]
    fn replacement_view_validation_order_matches_the_streams_contract() {
        let initial = descriptor(0, 2, ArrayBufferViewKind::Uint16, PullIntoReaderType::Byob);
        let view = |buffer_byte_length, byte_offset, byte_length, detached| ReplacementView {
            buffer_byte_length,
            byte_offset,
            byte_length,
            detached,
        };
        assert_eq!(
            initial.plan_respond_with_new_view(
                ReadableByteStreamState::Readable,
                view(0, 0, 0, true),
            ),
            Err(ReplacementViewError::Detached)
        );
        assert_eq!(
            initial.plan_respond_with_new_view(
                ReadableByteStreamState::Readable,
                view(0, 0, 0, false),
            ),
            Err(ReplacementViewError::EmptyWhileReadable)
        );
        assert_eq!(
            initial
                .plan_respond_with_new_view(ReadableByteStreamState::Closed, view(0, 8, 0, false),),
            Err(ReplacementViewError::BufferLengthMismatch)
        );
        assert_eq!(
            initial.plan_respond_with_new_view(
                ReadableByteStreamState::Readable,
                view(32, 9, 2, false),
            ),
            Err(ReplacementViewError::InvalidBounds)
        );
        assert_eq!(
            initial.plan_respond_with_new_view(
                ReadableByteStreamState::Closed,
                view(32, 8, 2, false),
            ),
            Err(ReplacementViewError::NonEmptyWhileClosed)
        );
    }

    #[test]
    fn close_rejects_only_a_partially_filled_typed_element() {
        assert_eq!(
            descriptor(1, 2, ArrayBufferViewKind::Uint16, PullIntoReaderType::Byob,)
                .validate_close(),
            Err(CloseError::PartiallyFilledElement)
        );
        assert_eq!(
            descriptor(2, 2, ArrayBufferViewKind::Uint16, PullIntoReaderType::Byob,)
                .validate_close(),
            Ok(())
        );
    }

    fn controller_snapshot(
        descriptors: Vec<Option<PullIntoDescriptor>>,
        queue_empty: bool,
        cached_byob_request: bool,
        auto_allocate_chunk_size: Option<usize>,
    ) -> ByteControllerSnapshot {
        ByteControllerSnapshot::new(
            crate::readable::ReadableSnapshot::new(
                crate::readable::ReadableState::Readable,
                false,
                queue_empty,
                0,
            ),
            descriptors,
            cached_byob_request,
            auto_allocate_chunk_size,
            None,
        )
    }

    #[test]
    fn byob_request_plan_preserves_cached_and_invalid_head_boundaries() {
        let byob = descriptor(2, 2, ArrayBufferViewKind::Uint16, PullIntoReaderType::Byob);
        assert_eq!(
            controller_snapshot(Vec::new(), true, false, None).plan_byob_request(),
            ByobRequestPlan::Unavailable
        );
        assert_eq!(
            controller_snapshot(vec![None, Some(byob)], true, false, None).plan_byob_request(),
            ByobRequestPlan::Unavailable
        );
        assert_eq!(
            controller_snapshot(vec![Some(byob)], true, true, None).plan_byob_request(),
            ByobRequestPlan::UseCached
        );

        let ByobRequestPlan::Create(plan) =
            controller_snapshot(vec![Some(byob)], true, false, None).plan_byob_request()
        else {
            panic!("a live head descriptor should create a BYOB request");
        };
        assert_eq!(plan.source(), byob);
        assert_eq!(plan.view(), byob.request_range());
    }

    #[test]
    fn pending_byob_view_requires_a_byob_owned_head() {
        let default = descriptor(
            0,
            1,
            ArrayBufferViewKind::Uint8,
            PullIntoReaderType::Default,
        );
        assert_eq!(
            controller_snapshot(vec![Some(default)], true, true, None).plan_pending_byob_view(),
            ByobRequestPlan::Unavailable
        );

        let byob = descriptor(0, 1, ArrayBufferViewKind::Uint8, PullIntoReaderType::Byob);
        assert_eq!(
            controller_snapshot(vec![Some(byob)], true, true, None).plan_pending_byob_view(),
            ByobRequestPlan::UseCached
        );
    }

    #[test]
    fn respond_context_joins_the_live_head_with_stream_lifecycle() {
        let byob = descriptor(0, 1, ArrayBufferViewKind::Uint8, PullIntoReaderType::Byob);
        let snapshot = |state, descriptors| {
            ByteControllerSnapshot::new(
                crate::readable::ReadableSnapshot::new(state, false, true, 1),
                descriptors,
                true,
                None,
                None,
            )
        };

        assert_eq!(
            snapshot(crate::readable::ReadableState::Readable, Vec::new()).respond_context(),
            None
        );
        assert_eq!(
            snapshot(
                crate::readable::ReadableState::Readable,
                vec![None, Some(byob)],
            )
            .respond_context(),
            None
        );

        let readable = snapshot(crate::readable::ReadableState::Readable, vec![Some(byob)])
            .respond_context()
            .unwrap();
        assert_eq!(readable.source(), byob);
        assert_eq!(
            readable.plan_respond(0),
            Err(RespondError::ZeroWhileReadable)
        );

        let closed = snapshot(crate::readable::ReadableState::Closed, vec![Some(byob)])
            .respond_context()
            .unwrap();
        assert_eq!(
            closed.plan_respond(0).unwrap().action(),
            RespondAction::Closed
        );
        assert_eq!(
            closed.plan_respond(1),
            Err(RespondError::NonZeroWhileClosed)
        );

        let errored = snapshot(crate::readable::ReadableState::Errored, vec![Some(byob)])
            .respond_context()
            .unwrap();
        assert_eq!(
            errored.plan_respond(0),
            Err(RespondError::ZeroWhileReadable)
        );
    }

    #[test]
    fn auto_allocation_plan_builds_the_default_uint8_descriptor() {
        assert_eq!(
            controller_snapshot(Vec::new(), true, false, None).plan_auto_allocate(),
            None
        );
        assert_eq!(
            controller_snapshot(Vec::new(), true, false, Some(0)).plan_auto_allocate(),
            None
        );

        let plan = controller_snapshot(Vec::new(), true, false, Some(8))
            .plan_auto_allocate()
            .expect("non-zero auto allocation should be planned");
        let descriptor = plan.descriptor();
        assert_eq!(descriptor.buffer_byte_length(), 8);
        assert_eq!(descriptor.byte_length(), 8);
        assert_eq!(descriptor.minimum_fill(), 1);
        assert_eq!(descriptor.view_kind(), ArrayBufferViewKind::Uint8);
        assert_eq!(descriptor.reader_type(), PullIntoReaderType::Default);
    }

    #[test]
    fn release_plan_retains_only_a_released_head_descriptor() {
        assert_eq!(
            controller_snapshot(Vec::new(), true, false, None).plan_release_reader(),
            ReleaseReaderPlan::None
        );

        let head = descriptor(3, 4, ArrayBufferViewKind::Uint16, PullIntoReaderType::Byob);
        assert_eq!(
            controller_snapshot(vec![Some(head)], true, false, None).plan_release_reader(),
            ReleaseReaderPlan::RetainReleasedHead {
                source: head,
                descriptor: head.released(),
            }
        );
    }

    #[test]
    fn close_plans_validate_the_head_only_after_queue_drain_and_wait_for_any_byob() {
        let partial = descriptor(1, 2, ArrayBufferViewKind::Uint16, PullIntoReaderType::Byob);
        assert_eq!(
            controller_snapshot(vec![Some(partial)], false, false, None).validate_close(),
            Ok(())
        );
        assert_eq!(
            controller_snapshot(vec![Some(partial)], true, false, None).validate_close(),
            Err(CloseError::PartiallyFilledElement)
        );

        let default = descriptor(
            0,
            1,
            ArrayBufferViewKind::Uint8,
            PullIntoReaderType::Default,
        );
        assert_eq!(
            controller_snapshot(vec![Some(default)], true, false, None).plan_finish_close(),
            FinishByteClosePlan::FinishDefaultReads
        );
        assert_eq!(
            controller_snapshot(vec![None, Some(partial), Some(default)], true, false, None,)
                .plan_finish_close(),
            FinishByteClosePlan::WaitForByobResponse
        );
    }

    #[test]
    fn enqueue_start_partitions_lifecycle_pipe_and_controller_paths() {
        let snapshot = |state, close_requested, pipe_read_pending| {
            ByteControllerSnapshot::new(
                crate::readable::ReadableSnapshot::new(state, close_requested, true, 0),
                Vec::new(),
                false,
                None,
                pipe_read_pending,
            )
        };
        assert_eq!(
            snapshot(crate::readable::ReadableState::Closed, false, Some(true))
                .plan_enqueue_start(),
            ByteEnqueueStartPlan::Reject
        );
        assert_eq!(
            snapshot(crate::readable::ReadableState::Readable, true, Some(true))
                .plan_enqueue_start(),
            ByteEnqueueStartPlan::Reject
        );
        assert_eq!(
            snapshot(crate::readable::ReadableState::Readable, false, None).plan_enqueue_start(),
            ByteEnqueueStartPlan::Continue
        );

        let ByteEnqueueStartPlan::Pipe(pending) =
            snapshot(crate::readable::ReadableState::Readable, false, Some(true))
                .plan_enqueue_start()
        else {
            panic!("a pipe-owned pending read should select the pipe route");
        };
        assert!(pending.fulfills_pending_read());
        assert_eq!(pending.queue_size(9), 0);

        let ByteEnqueueStartPlan::Pipe(buffered) =
            snapshot(crate::readable::ReadableState::Readable, false, Some(false))
                .plan_enqueue_start()
        else {
            panic!("an attached pipe should select the pipe route");
        };
        assert!(!buffered.fulfills_pending_read());
        assert_eq!(buffered.queue_size(9), 9);
    }

    #[test]
    fn enqueue_continuation_uses_the_live_head_reader_owner() {
        let default = descriptor(
            0,
            1,
            ArrayBufferViewKind::Uint8,
            PullIntoReaderType::Default,
        );
        assert_eq!(
            controller_snapshot(vec![Some(default)], true, false, None)
                .plan_enqueue_after_released(),
            ByteEnqueueContinuationPlan::FulfillDefaultDescriptor { source: default }
        );

        let byob = descriptor(0, 1, ArrayBufferViewKind::Uint8, PullIntoReaderType::Byob);
        assert_eq!(
            controller_snapshot(vec![Some(byob)], true, false, None).plan_enqueue_after_released(),
            ByteEnqueueContinuationPlan::QueueWithByobDescriptor { source: byob }
        );
        assert_eq!(
            controller_snapshot(vec![Some(byob.released())], true, false, None)
                .plan_enqueue_after_released(),
            ByteEnqueueContinuationPlan::Queue
        );
        assert_eq!(
            controller_snapshot(Vec::new(), true, false, None).plan_enqueue_after_released(),
            ByteEnqueueContinuationPlan::Queue
        );
    }

    #[test]
    fn descriptor_resolution_preserves_default_done_and_byob_view_shapes() {
        let default = descriptor(
            4,
            1,
            ArrayBufferViewKind::Uint8,
            PullIntoReaderType::Default,
        );
        assert_eq!(
            default.plan_resolution(true).result(),
            DescriptorResultPlan::Undefined
        );
        assert_eq!(
            default.plan_resolution(false).result(),
            DescriptorResultPlan::View {
                kind: ArrayBufferViewKind::Uint8,
                range: ByteRange::new(8, 4),
            }
        );

        let byob = descriptor(4, 2, ArrayBufferViewKind::Uint16, PullIntoReaderType::Byob);
        let closed = byob.plan_resolution(true);
        assert!(closed.done());
        assert_eq!(closed.source(), byob);
        assert_eq!(
            closed.result(),
            DescriptorResultPlan::View {
                kind: ArrayBufferViewKind::Uint16,
                range: ByteRange::new(8, 4),
            }
        );
    }

    #[test]
    fn read_into_plan_partitions_lifecycle_and_existing_descriptor_ownership() {
        let snapshot = |state, descriptors| {
            ByteControllerSnapshot::new(
                crate::readable::ReadableSnapshot::new(state, false, true, 0),
                descriptors,
                false,
                None,
                None,
            )
        };
        assert_eq!(
            snapshot(crate::readable::ReadableState::Errored, Vec::new()).plan_read_into(),
            ByteReadIntoPlan::RejectStoredError
        );
        assert_eq!(
            snapshot(crate::readable::ReadableState::Closed, Vec::new()).plan_read_into(),
            ByteReadIntoPlan::ResolveDone
        );
        assert_eq!(
            snapshot(crate::readable::ReadableState::Readable, Vec::new()).plan_read_into(),
            ByteReadIntoPlan::FillFromQueue
        );

        let head = descriptor(0, 1, ArrayBufferViewKind::Uint8, PullIntoReaderType::Byob);
        assert_eq!(
            snapshot(crate::readable::ReadableState::Readable, vec![Some(head)],).plan_read_into(),
            ByteReadIntoPlan::AppendBehindHead { source: head }
        );
    }

    #[test]
    fn head_resolution_rejects_missing_released_and_unaligned_descriptors() {
        assert_eq!(
            controller_snapshot(Vec::new(), true, false, None).plan_take_head_resolution(false),
            Err(HeadResolutionError::MissingDescriptor)
        );

        let byob = descriptor(0, 2, ArrayBufferViewKind::Uint16, PullIntoReaderType::Byob);
        assert_eq!(
            controller_snapshot(vec![Some(byob.released())], true, false, None)
                .plan_take_head_resolution(false),
            Err(HeadResolutionError::ReleasedDescriptor)
        );

        let unaligned = descriptor(1, 2, ArrayBufferViewKind::Uint16, PullIntoReaderType::Byob);
        assert_eq!(
            controller_snapshot(vec![Some(unaligned)], true, false, None)
                .plan_take_head_resolution(false),
            Err(HeadResolutionError::UnalignedDescriptor)
        );
        assert_eq!(
            controller_snapshot(vec![Some(byob)], true, false, None)
                .plan_take_head_resolution(false),
            Ok(byob.plan_resolution(false))
        );
    }

    #[test]
    fn default_read_step_requires_no_descriptor_a_pending_read_and_queued_data() {
        let snapshot = |pending_read_count, queue_empty, descriptors| {
            ByteControllerSnapshot::new(
                crate::readable::ReadableSnapshot::new(
                    crate::readable::ReadableState::Readable,
                    false,
                    queue_empty,
                    pending_read_count,
                ),
                descriptors,
                false,
                None,
                None,
            )
        };
        assert_eq!(
            snapshot(1, false, Vec::new()).plan_default_read_step(),
            DefaultReadStepPlan::Dequeue
        );
        assert_eq!(
            snapshot(0, false, Vec::new()).plan_default_read_step(),
            DefaultReadStepPlan::Wait
        );
        assert_eq!(
            snapshot(1, true, Vec::new()).plan_default_read_step(),
            DefaultReadStepPlan::Wait
        );

        let head = descriptor(0, 1, ArrayBufferViewKind::Uint8, PullIntoReaderType::Byob);
        assert_eq!(
            snapshot(1, false, vec![Some(head)]).plan_default_read_step(),
            DefaultReadStepPlan::Wait
        );
    }

    #[test]
    fn released_head_plans_distinguish_open_flush_from_closed_removal() {
        let active = descriptor(3, 4, ArrayBufferViewKind::Uint16, PullIntoReaderType::Byob);
        assert_eq!(
            controller_snapshot(vec![Some(active)], true, false, None).plan_flush_released_head(),
            FlushReleasedHeadPlan::Done
        );
        assert_eq!(
            controller_snapshot(vec![Some(active)], true, false, None).plan_closed_released_head(),
            ClosedReleasedHeadPlan::Continue
        );

        let released = active.released();
        assert_eq!(
            controller_snapshot(vec![Some(released)], true, false, None).plan_flush_released_head(),
            FlushReleasedHeadPlan::Flush {
                source: released,
                bytes: ByteRange::new(8, 3),
            }
        );
        assert_eq!(
            controller_snapshot(vec![Some(released)], true, false, None)
                .plan_closed_released_head(),
            ClosedReleasedHeadPlan::Remove { source: released }
        );
    }

    #[test]
    fn closed_byob_step_resolves_only_aligned_byob_owned_pending_reads() {
        let byob = descriptor(4, 2, ArrayBufferViewKind::Uint16, PullIntoReaderType::Byob);
        assert_eq!(
            controller_snapshot(vec![Some(byob)], true, false, None).plan_closed_byob_step(0),
            Ok(ClosedByobStepPlan::Done)
        );
        assert_eq!(
            controller_snapshot(vec![Some(byob)], true, false, None).plan_closed_byob_step(1),
            Ok(ClosedByobStepPlan::Resolve(byob.plan_resolution(true)))
        );

        let default = descriptor(
            0,
            1,
            ArrayBufferViewKind::Uint8,
            PullIntoReaderType::Default,
        );
        assert_eq!(
            controller_snapshot(vec![Some(default)], true, false, None).plan_closed_byob_step(1),
            Ok(ClosedByobStepPlan::Done)
        );

        let unaligned = descriptor(1, 2, ArrayBufferViewKind::Uint16, PullIntoReaderType::Byob);
        assert_eq!(
            controller_snapshot(vec![Some(unaligned)], true, false, None).plan_closed_byob_step(1),
            Err(HeadResolutionError::UnalignedDescriptor)
        );
    }
}
