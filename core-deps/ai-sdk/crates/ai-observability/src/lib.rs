//! Observability: structured events, chronological execution history,
//! span/trace correlation, and durable exporters.
//!
//! Every significant AI execution produces [`ExecutionEvent`]s with
//! RFC 3339 timestamps, trace/span ids, durations, and typed metadata.
//! Events are collected in-process (bounded), exported durably through an
//! [`EventSink`], and rendered as a chronological report (spec §14) — as
//! structured telemetry, not expensive string logging.
//!
//! Correlation: emitters share a [`TraceContext`] across an execution so
//! one run produces ONE multi-event trace (`EventCollector::open_span`,
//! `EventCollector::record_in_trace`). Persistence: [`EventSink::export`]
//! returns a [`Result`](std::result::Result) and
//! [`EventCollector::try_flush`] clears the in-memory buffer only after
//! every sink accepted the batch, so I/O failures never destroy events.
//! Round trips are lossless: serializing events to JSONL and loading them
//! back with [`ExecutionEvent::from_jsonl`] +
//! [`EventCollector::insert_event`] preserves `wall_time` and `offset_ms`
//! exactly.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// What kind of operation an event describes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    RequestStarted,
    ProviderCall,
    ModelCall,
    ToolCall,
    AgentStep,
    AgentState,
    WorkflowStep,
    WebRequest,
    MemoryOperation,
    Retry,
    Fallback,
    Error,
    Completed,
    Metric,
}

/// Lifecycle status of the operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventStatus {
    Started,
    Succeeded,
    Failed,
    Retrying,
    Cancelled,
}

/// A single structured execution event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExecutionEvent {
    /// Wall-clock time in RFC 3339 (UTC), e.g. `2025-01-01T12:00:00.123Z`.
    pub wall_time: String,
    /// Monotonic offset from the collector's start (for chronological
    /// reports).
    pub offset_ms: u64,
    pub trace_id: String,
    pub span_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_span_id: Option<String>,
    pub kind: EventKind,
    pub operation: String,
    pub status: EventStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(default)]
    pub metadata: BTreeMap<String, serde_json::Value>,
}

impl ExecutionEvent {
    pub fn started(&self) -> bool {
        self.status == EventStatus::Started
    }

    /// Parses one newline-delimited JSON event (the inverse of
    /// [`ExecutionEvent::to_jsonl`]). All fields, including `wall_time` and
    /// `offset_ms`, survive intact.
    pub fn from_jsonl(line: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(line.trim())
    }

    /// Serializes this event as one JSONL line (no trailing newline).
    pub fn to_jsonl(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }
}

/// Current wall-clock time formatted as an RFC 3339 UTC timestamp.
///
/// The fallback is unreachable in practice: `Rfc3339` formatting only fails
/// for years outside `0..=9999`.
pub fn wall_clock_now() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

/// Correlation identity shared by every event of one execution: a single
/// trace id plus its root span id. Mint once per run (or restore from a
/// serialized form with [`TraceContext::from_ids`]) and pass it to
/// [`EventCollector::record_in_trace`] / [`EventCollector::open_span`] so
/// all events of the run land in one trace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceContext {
    trace_id: String,
    root_span_id: String,
}

impl TraceContext {
    /// Mints a fresh trace id and root span id.
    pub fn new() -> Self {
        Self {
            trace_id: Uuid::new_v4().to_string(),
            root_span_id: Uuid::new_v4().to_string(),
        }
    }

    /// Reuses caller-supplied ids (e.g. propagated from an upstream caller).
    pub fn from_ids(trace_id: impl Into<String>, root_span_id: impl Into<String>) -> Self {
        Self {
            trace_id: trace_id.into(),
            root_span_id: root_span_id.into(),
        }
    }

    pub fn trace_id(&self) -> &str {
        &self.trace_id
    }

    pub fn root_span_id(&self) -> &str {
        &self.root_span_id
    }

    /// Mints a fresh span id belonging to this trace.
    pub fn new_span_id(&self) -> String {
        Uuid::new_v4().to_string()
    }
}

impl Default for TraceContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Error returned when an [`EventSink`] fails to persist a batch of events.
#[derive(Debug, Clone, thiserror::Error)]
#[error("event export failed: {message}")]
pub struct ExportError {
    message: String,
}

impl ExportError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl From<std::io::Error> for ExportError {
    fn from(error: std::io::Error) -> Self {
        Self::new(error.to_string())
    }
}

/// Receives exported events durably (files, stdout, remote backends).
///
/// Unlike [`EventExporter`] (best-effort, infallible), an `EventSink`
/// propagates failures so [`EventCollector::try_flush`] can keep the
/// buffered events for a later retry instead of dropping them.
pub trait EventSink: Send + Sync {
    /// Persists one batch of events. Return an error if (and only if) the
    /// batch was NOT fully persisted.
    fn export(&self, events: &[ExecutionEvent]) -> Result<(), ExportError>;
}

/// Receives exported events best-effort (legacy, infallible interface).
///
/// Prefer [`EventSink`] for anything that must not silently lose events.
pub trait EventExporter: Send + Sync {
    fn export(&self, events: &[ExecutionEvent]);
}

impl<E: EventExporter + ?Sized> EventExporter for Arc<E> {
    fn export(&self, events: &[ExecutionEvent]) {
        (**self).export(events);
    }
}

/// Wraps any [`EventExporter`] so it can be attached where an
/// [`EventSink`] is expected. Export through this adapter never fails
/// (the wrapped exporter is best-effort by contract).
pub struct ExporterAsSink<E: EventExporter> {
    exporter: E,
}

impl<E: EventExporter> ExporterAsSink<E> {
    pub fn new(exporter: E) -> Self {
        Self { exporter }
    }
}

impl<E: EventExporter> EventSink for ExporterAsSink<E> {
    fn export(&self, events: &[ExecutionEvent]) -> Result<(), ExportError> {
        self.exporter.export(events);
        Ok(())
    }
}

/// Writer adapter that turns `flush` into `sync_all`, giving
/// [`JsonLinesExporter`] fsync-per-batch durability with plain
/// `std::fs::File`s.
#[derive(Debug)]
pub struct FsyncFile {
    file: std::fs::File,
}

impl FsyncFile {
    /// Opens (creating or truncating) a file for writing; every flush
    /// fsyncs.
    pub fn create(path: impl AsRef<Path>) -> std::io::Result<Self> {
        Ok(Self {
            file: std::fs::OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(path)?,
        })
    }

    /// Opens a file for appending (creating if absent); every flush fsyncs.
    pub fn append(path: impl AsRef<Path>) -> std::io::Result<Self> {
        Ok(Self {
            file: std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)?,
        })
    }

    pub fn get_ref(&self) -> &std::fs::File {
        &self.file
    }
}

impl std::io::Write for FsyncFile {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.file.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.file.sync_all()
    }
}

/// Writes events as newline-delimited JSON.
///
/// Through [`EventSink`] every serialization/write/flush error is
/// propagated ([`FsyncFile`] writers additionally fsync on flush).
/// The legacy [`EventExporter`] implementation stays best-effort but logs
/// failures instead of swallowing them silently.
pub struct JsonLinesExporter<W: std::io::Write + Send> {
    writer: parking_lot::Mutex<W>,
}

impl<W: std::io::Write + Send> JsonLinesExporter<W> {
    pub fn new(writer: W) -> Self {
        Self {
            writer: parking_lot::Mutex::new(writer),
        }
    }
}

impl JsonLinesExporter<FsyncFile> {
    /// Creates a JSONL exporter over a freshly created/truncated file with
    /// fsync-per-batch durability.
    pub fn create_file(path: impl AsRef<Path>) -> std::io::Result<Self> {
        Ok(Self::new(FsyncFile::create(path)?))
    }

    /// Creates a JSONL exporter appending to (or creating) a file with
    /// fsync-per-batch durability.
    pub fn append_file(path: impl AsRef<Path>) -> std::io::Result<Self> {
        Ok(Self::new(FsyncFile::append(path)?))
    }
}

impl<W: std::io::Write + Send> JsonLinesExporter<W> {
    /// Serializes and writes one batch of events, flushing afterwards.
    /// All errors propagate; on error the batch must be retried by the
    /// caller.
    pub fn write_events(&self, events: &[ExecutionEvent]) -> Result<(), ExportError> {
        let mut writer = self.writer.lock();
        for event in events {
            let line = event
                .to_jsonl()
                .map_err(|e| ExportError::new(e.to_string()))?;
            writeln!(writer, "{line}").map_err(ExportError::from)?;
        }
        writer.flush().map_err(ExportError::from)?;
        Ok(())
    }
}

impl<W: std::io::Write + Send> EventSink for JsonLinesExporter<W> {
    fn export(&self, events: &[ExecutionEvent]) -> Result<(), ExportError> {
        self.write_events(events)
    }
}

impl<W: std::io::Write + Send> EventExporter for JsonLinesExporter<W> {
    fn export(&self, events: &[ExecutionEvent]) {
        if let Err(e) = self.write_events(events) {
            tracing::warn!("json-lines export lost {} events: {e}", events.len());
        }
    }
}

/// Collects execution events with bounded retention.
///
/// `capacity` bounds memory (spec: bounded resources). Events beyond the
/// capacity drop the oldest.
#[derive(Clone)]
pub struct EventCollector {
    inner: Arc<RwLock<Inner>>,
}

struct Inner {
    events: Vec<ExecutionEvent>,
    capacity: usize,
    start: Instant,
}

impl Default for EventCollector {
    fn default() -> Self {
        Self::with_capacity(10_000)
    }
}

impl EventCollector {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            inner: Arc::new(RwLock::new(Inner {
                events: Vec::new(),
                capacity,
                start: Instant::now(),
            })),
        }
    }

    /// Records an event, returning its span id.
    ///
    /// Each call mints its own trace id; use [`EventCollector::record_in_trace`]
    /// or [`EventCollector::open_span`] to correlate multiple events into
    /// one trace.
    pub fn record(
        &self,
        kind: EventKind,
        operation: impl Into<String>,
        status: EventStatus,
        metadata: BTreeMap<String, serde_json::Value>,
    ) -> String {
        let span_id = Uuid::new_v4().to_string();
        let trace_id = Uuid::new_v4().to_string();
        self.record_with_ids(
            kind,
            operation,
            status,
            metadata,
            trace_id,
            span_id.clone(),
            None,
            None,
        );
        span_id
    }

    #[allow(clippy::too_many_arguments)]
    pub fn record_with_ids(
        &self,
        kind: EventKind,
        operation: impl Into<String>,
        status: EventStatus,
        metadata: BTreeMap<String, serde_json::Value>,
        trace_id: String,
        span_id: String,
        parent_span_id: Option<String>,
        duration_ms: Option<u64>,
    ) {
        let event = ExecutionEvent {
            wall_time: wall_clock_now(),
            offset_ms: self.offset_now(),
            trace_id,
            span_id,
            parent_span_id,
            kind,
            operation: operation.into(),
            status,
            duration_ms,
            metadata,
        };
        self.insert_event(event);
    }

    /// Records one event inside an existing trace, minting a fresh span id
    /// for it. Returns that span id.
    #[allow(clippy::too_many_arguments)]
    pub fn record_in_trace(
        &self,
        ctx: &TraceContext,
        kind: EventKind,
        operation: impl Into<String>,
        status: EventStatus,
        parent_span_id: Option<String>,
        duration_ms: Option<u64>,
    ) -> String {
        let span_id = ctx.new_span_id();
        self.record_with_ids(
            kind,
            operation,
            status,
            BTreeMap::new(),
            ctx.trace_id().to_string(),
            span_id.clone(),
            parent_span_id,
            duration_ms,
        );
        span_id
    }

    /// Opens a timed span inside an existing trace. Dropping the guard (or
    /// calling [`EventGuard::finish`]) records the span's completion event
    /// with its measured duration.
    pub fn open_span(
        &self,
        ctx: &TraceContext,
        kind: EventKind,
        operation: impl Into<String>,
        parent_span_id: Option<String>,
    ) -> EventGuard {
        let span_id = ctx.new_span_id();
        EventGuard::new(
            self,
            kind,
            operation,
            ctx.trace_id().to_string(),
            span_id,
            parent_span_id,
        )
    }

    /// Inserts a fully-formed event verbatim (bounded retention applies).
    /// This is the lossless load path: `wall_time` and `offset_ms` are kept
    /// exactly as provided.
    pub fn insert_event(&self, event: ExecutionEvent) {
        let mut inner = self.inner.write();
        if inner.events.len() >= inner.capacity {
            inner.events.remove(0);
        }
        inner.events.push(event);
    }

    /// Bulk-inserts deserialized events verbatim, preserving their original
    /// order, `wall_time`s, and `offset_ms` (see
    /// [`ExecutionEvent::from_jsonl`]).
    pub fn load_events<I: IntoIterator<Item = ExecutionEvent>>(&self, events: I) {
        for event in events {
            self.insert_event(event);
        }
    }

    fn offset_now(&self) -> u64 {
        self.inner.read().start.elapsed().as_millis() as u64
    }

    /// All events, oldest first.
    pub fn events(&self) -> Vec<ExecutionEvent> {
        self.inner.read().events.clone()
    }

    /// Events for a given trace.
    pub fn trace(&self, trace_id: &str) -> Vec<ExecutionEvent> {
        self.inner
            .read()
            .events
            .iter()
            .filter(|e| e.trace_id == trace_id)
            .cloned()
            .collect()
    }

    /// Exports all buffered events through every exporter and clears the
    /// buffer only after every exporter received the full batch.
    ///
    /// Note: [`EventExporter`] is best-effort (infallible); for guaranteed
    /// delivery use [`EventCollector::try_flush`] with an [`EventSink`].
    pub fn flush(&self, exporters: &[Arc<dyn EventExporter>]) {
        if exporters.is_empty() {
            return;
        }
        let events = self.snapshot();
        if events.is_empty() {
            return;
        }
        for exporter in exporters {
            exporter.export(&events);
        }
        // Clear only the exported prefix; events appended concurrently stay.
        let mut inner = self.inner.write();
        let n = events.len().min(inner.events.len());
        inner.events.drain(0..n);
    }

    /// Durably exports all buffered events through every sink, clearing the
    /// buffer only after EVERY sink accepted the full batch. On any error
    /// the buffer is left intact for a later retry and the error is
    /// returned.
    pub fn try_flush(&self, sinks: &[Arc<dyn EventSink>]) -> Result<(), ExportError> {
        if sinks.is_empty() {
            return Ok(());
        }
        let events = self.snapshot();
        if events.is_empty() {
            return Ok(());
        }
        for sink in sinks {
            sink.export(&events)?;
        }
        let mut inner = self.inner.write();
        let n = events.len().min(inner.events.len());
        inner.events.drain(0..n);
        Ok(())
    }

    fn snapshot(&self) -> Vec<ExecutionEvent> {
        self.inner.read().events.clone()
    }

    pub fn len(&self) -> usize {
        self.inner.read().events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Renders events as a chronological execution report (spec §14):
///
/// ```text
/// 00:00.000 Request received
/// 00:00.003 Agent initialized
/// ...
/// ```
pub fn chronological_report(events: &[ExecutionEvent]) -> String {
    let mut sorted = events.to_vec();
    sorted.sort_by_key(|e| e.offset_ms);

    let mut out = String::new();
    for event in sorted {
        let secs = event.offset_ms / 1000;
        let millis = event.offset_ms % 1000;
        out.push_str(&format!("{:02}:{:02}.{:03} ", secs / 60, secs % 60, millis));
        match event.status {
            EventStatus::Started => out.push_str("▶ "),
            EventStatus::Succeeded => out.push_str("✓ "),
            EventStatus::Failed => out.push_str("✗ "),
            EventStatus::Retrying => out.push_str("↻ "),
            EventStatus::Cancelled => out.push_str("⏹ "),
        }
        out.push_str(&format!("{:?} {}", event.kind, event.operation));
        if let Some(d) = event.duration_ms {
            out.push_str(&format!(" ({d} ms)"));
        }
        out.push('\n');
    }
    out
}

/// A timing guard that records a completed event on drop (structured, not
/// string-logged). Create via [`EventCollector::open_span`] (shares a trace
/// context) or [`EventGuard::new`]; nest further spans with
/// [`EventGuard::child`].
pub struct EventGuard {
    collector: EventCollector,
    kind: EventKind,
    operation: String,
    trace_id: String,
    span_id: String,
    parent_span_id: Option<String>,
    started: Instant,
    metadata: BTreeMap<String, serde_json::Value>,
    finished: bool,
}

impl EventGuard {
    pub fn new(
        collector: &EventCollector,
        kind: EventKind,
        operation: impl Into<String>,
        trace_id: String,
        span_id: String,
        parent_span_id: Option<String>,
    ) -> Self {
        Self {
            collector: collector.clone(),
            kind,
            operation: operation.into(),
            trace_id,
            span_id,
            parent_span_id,
            started: Instant::now(),
            metadata: BTreeMap::new(),
            finished: false,
        }
    }

    /// Opens a nested span whose parent is this guard's span (same trace).
    pub fn child(&self, kind: EventKind, operation: impl Into<String>) -> EventGuard {
        let span_id = uuid::Uuid::new_v4().to_string();
        EventGuard::new(
            &self.collector,
            kind,
            operation,
            self.trace_id.clone(),
            span_id,
            Some(self.span_id.clone()),
        )
    }

    pub fn with_metadata(mut self, key: &str, value: serde_json::Value) -> Self {
        self.metadata.insert(key.to_string(), value);
        self
    }

    pub fn span_id(&self) -> &str {
        &self.span_id
    }

    pub fn trace_id(&self) -> &str {
        &self.trace_id
    }

    /// Status recorded when the guard drops without an explicit `finish`:
    /// [`EventStatus::Failed`] while unwinding a panic,
    /// [`EventStatus::Succeeded`] otherwise.
    pub fn drop_status(panicking: bool) -> EventStatus {
        if panicking {
            EventStatus::Failed
        } else {
            EventStatus::Succeeded
        }
    }

    /// Records the final event for this span. Idempotent: a dropped guard
    /// after an explicit `finish` records nothing.
    pub fn finish(mut self, status: EventStatus) {
        if !self.finished {
            self.finished = true;
            self.record(status);
        }
    }

    fn record(&self, status: EventStatus) {
        let duration_ms = self.started.elapsed().as_millis() as u64;
        self.collector.record_with_ids(
            self.kind,
            &self.operation,
            status,
            self.metadata.clone(),
            self.trace_id.clone(),
            self.span_id.clone(),
            self.parent_span_id.clone(),
            Some(duration_ms),
        );
    }
}

impl Drop for EventGuard {
    fn drop(&mut self) {
        // Record a completion event only if the guard was not explicitly
        // finished. A scope abandoned by panic unwinding is recorded as
        // Failed, never as Succeeded.
        if !self.finished {
            self.finished = true;
            self.record(Self::drop_status(std::thread::panicking()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn collector_records_and_bounds() {
        let collector = EventCollector::with_capacity(3);
        for i in 0..5 {
            collector.record(
                EventKind::ProviderCall,
                format!("call-{i}"),
                EventStatus::Succeeded,
                BTreeMap::new(),
            );
        }
        assert_eq!(collector.len(), 3, "capacity bounds memory");
        let events = collector.events();
        assert_eq!(events[0].operation, "call-2");
    }

    #[test]
    fn chronological_report_is_ordered() {
        let collector = EventCollector::new();
        collector.record(
            EventKind::RequestStarted,
            "request",
            EventStatus::Started,
            BTreeMap::new(),
        );
        collector.record(
            EventKind::ModelCall,
            "openai:gpt-4o",
            EventStatus::Succeeded,
            BTreeMap::new(),
        );
        collector.record(
            EventKind::Completed,
            "request",
            EventStatus::Succeeded,
            BTreeMap::new(),
        );

        let report = chronological_report(&collector.events());
        assert!(report.contains("▶ RequestStarted request"), "{report}");
        assert!(report.contains("✓ ModelCall"), "{report}");
        assert!(report.contains("00:00."), "{report}");
    }

    #[test]
    fn trace_filtering_works() {
        let collector = EventCollector::new();
        let span = collector.record(
            EventKind::AgentStep,
            "step-1",
            EventStatus::Started,
            BTreeMap::new(),
        );
        let events = collector.events();
        let trace_id = events[0].trace_id.clone();
        assert_eq!(collector.trace(&trace_id).len(), 1);
        assert_eq!(collector.trace("nope").len(), 0);
        assert!(!span.is_empty());
    }

    #[test]
    fn guard_records_duration() {
        let collector = EventCollector::new();
        {
            let guard = EventGuard::new(
                &collector,
                EventKind::ToolCall,
                "calculator",
                "t".into(),
                "s".into(),
                None,
            );
            std::thread::sleep(Duration::from_millis(5));
            guard.finish(EventStatus::Succeeded);
        }
        let events = collector.events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, EventKind::ToolCall);
        assert!(events[0].duration_ms.unwrap_or(0) >= 5);
    }

    #[test]
    fn json_lines_exporter_writes_valid_lines() {
        let collector = EventCollector::new();
        collector.record(
            EventKind::Metric,
            "tokens",
            EventStatus::Succeeded,
            BTreeMap::new(),
        );

        let dir = std::env::temp_dir();
        let path = dir.join(format!("ai-sdk-events-{}.jsonl", std::process::id()));
        let file = std::fs::File::create(&path).expect("event file created");
        let exporter = Arc::new(JsonLinesExporter::new(file));
        collector.flush(&[exporter]);
        assert!(collector.is_empty(), "flush clears the buffer");

        let text = std::fs::read_to_string(&path).expect("event file readable");
        std::fs::remove_file(&path).ok();
        let parsed: serde_json::Value = serde_json::from_str(text.trim()).unwrap();
        assert_eq!(parsed["kind"], "metric");
    }

    #[test]
    fn wall_time_is_rfc3339_utc() {
        let collector = EventCollector::new();
        collector.record(
            EventKind::Metric,
            "clock",
            EventStatus::Succeeded,
            BTreeMap::new(),
        );
        let wall_time = collector.events()[0].wall_time.clone();
        let parsed =
            time::OffsetDateTime::parse(&wall_time, &time::format_description::well_known::Rfc3339)
                .unwrap_or_else(|e| panic!("wall_time `{wall_time}` is not RFC 3339: {e}"));
        assert!(
            parsed.offset().is_utc(),
            "expected UTC offset, got `{wall_time}`"
        );
    }

    #[test]
    fn record_in_trace_correlates_events() {
        let collector = EventCollector::new();
        let ctx = TraceContext::new();
        let root = collector.open_span(&ctx, EventKind::RequestStarted, "request", None);
        let root_span_id = root.span_id().to_string();

        // One-off event inside the same trace (as emitters do for Retry).
        let one_off = collector.record_in_trace(
            &ctx,
            EventKind::Retry,
            "backoff",
            EventStatus::Retrying,
            Some(root_span_id.clone()),
            None,
        );

        let child_op = root.child(EventKind::ModelCall, "generate");
        let child_span_id = child_op.span_id().to_string();
        child_op.finish(EventStatus::Succeeded);
        root.finish(EventStatus::Succeeded);

        let trace = collector.trace(ctx.trace_id());
        assert_eq!(trace.len(), 3, "all three events share one trace");
        assert_eq!(
            trace.iter().map(|e| e.span_id.as_str()).collect::<Vec<_>>(),
            vec![
                one_off.as_str(),
                child_span_id.as_str(),
                root_span_id.as_str()
            ],
        );
        assert_eq!(
            trace[0].parent_span_id.as_deref(),
            Some(root_span_id.as_str()),
            "one-off events nest under the opened span"
        );
        assert_eq!(
            trace[1].parent_span_id.as_deref(),
            Some(root_span_id.as_str()),
            "child spans nest under the opened span"
        );
        assert_eq!(trace[2].parent_span_id, None, "opened span has no parent");

        let offsets: Vec<u64> = trace.iter().map(|e| e.offset_ms).collect();
        assert!(
            offsets.windows(2).all(|w| w[0] <= w[1]),
            "offsets are non-decreasing: {offsets:?}"
        );
    }

    #[test]
    fn insert_event_preserves_fields_verbatim() {
        let collector = EventCollector::new();
        let original = ExecutionEvent {
            wall_time: "2001-02-03T04:05:06.789Z".into(),
            offset_ms: 4242,
            trace_id: "fixed-trace".into(),
            span_id: "fixed-span".into(),
            parent_span_id: Some("parent".into()),
            kind: EventKind::ToolCall,
            operation: "op".into(),
            status: EventStatus::Cancelled,
            duration_ms: Some(17),
            metadata: BTreeMap::from([("k".into(), serde_json::json!("v"))]),
        };
        collector.insert_event(original.clone());
        assert_eq!(collector.events(), vec![original]);
    }

    #[test]
    fn jsonl_round_trip_is_lossless() {
        let originals = vec![
            ExecutionEvent {
                wall_time: "1999-12-31T23:59:58.000000001Z".into(),
                offset_ms: 0,
                trace_id: "t".into(),
                span_id: "s0".into(),
                parent_span_id: None,
                kind: EventKind::RequestStarted,
                operation: "request".into(),
                status: EventStatus::Started,
                duration_ms: None,
                metadata: BTreeMap::new(),
            },
            ExecutionEvent {
                wall_time: "2000-01-01T00:00:00.5Z".into(),
                offset_ms: 1500,
                trace_id: "t".into(),
                span_id: "s1".into(),
                parent_span_id: Some("s0".into()),
                kind: EventKind::ModelCall,
                operation: "gpt".into(),
                status: EventStatus::Succeeded,
                duration_ms: Some(42),
                metadata: BTreeMap::from([("tokens".into(), serde_json::json!(11))]),
            },
        ];
        let text = originals
            .iter()
            .map(|e| e.to_jsonl().unwrap())
            .collect::<Vec<_>>()
            .join("\n");

        let loaded = EventCollector::new();
        for line in text.lines() {
            loaded.insert_event(ExecutionEvent::from_jsonl(line).unwrap());
        }
        assert_eq!(loaded.events(), originals);
        assert_eq!(loaded.events()[1].wall_time, "2000-01-01T00:00:00.5Z");
        assert_eq!(loaded.events()[1].offset_ms, 1500);
    }

    #[derive(Debug, Clone, Copy)]
    enum SinkBehaviour {
        Accept,
        Reject,
    }

    #[derive(Debug)]
    struct BehaviourSink(SinkBehaviour);

    impl EventSink for BehaviourSink {
        fn export(&self, events: &[ExecutionEvent]) -> Result<(), ExportError> {
            match self.0 {
                SinkBehaviour::Accept => Ok(()),
                SinkBehaviour::Reject => Err(ExportError::new(format!(
                    "simulated failure for {} events",
                    events.len()
                ))),
            }
        }
    }

    #[test]
    fn try_flush_clears_buffer_only_after_success() {
        let collector = EventCollector::new();
        collector.record(
            EventKind::Metric,
            "keep-me",
            EventStatus::Succeeded,
            BTreeMap::new(),
        );
        let failing: Arc<dyn EventSink> = Arc::new(BehaviourSink(SinkBehaviour::Reject));
        let err = collector.try_flush(&[failing]).unwrap_err();
        assert!(err.to_string().contains("simulated failure"), "{err}");
        assert_eq!(collector.len(), 1, "failed export keeps events buffered");

        let good: Arc<dyn EventSink> = Arc::new(BehaviourSink(SinkBehaviour::Accept));
        collector.try_flush(&[good]).expect("retry succeeds");
        assert!(collector.is_empty(), "successful export drains the buffer");
    }

    #[test]
    fn flush_exports_before_clearing() {
        let collector = EventCollector::new();
        collector.record(
            EventKind::Metric,
            "visible",
            EventStatus::Succeeded,
            BTreeMap::new(),
        );
        // The exporter observes the buffer while it runs: events must still
        // be present until every exporter got the batch.
        struct Observe {
            seen_len: parking_lot::Mutex<Option<usize>>,
        }
        impl EventExporter for Observe {
            fn export(&self, events: &[ExecutionEvent]) {
                *self.seen_len.lock() = Some(events.len());
            }
        }
        let observe = Arc::new(Observe {
            seen_len: parking_lot::Mutex::new(None),
        });
        let exporters: [Arc<dyn EventExporter>; 1] = [observe.clone()];
        collector.flush(&exporters);
        assert_eq!(*observe.seen_len.lock(), Some(1));
        assert!(collector.is_empty());
    }

    struct AlwaysFailsWriter;

    impl std::io::Write for AlwaysFailsWriter {
        fn write(&mut self, _buf: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("disk gone"))
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Err(std::io::Error::other("disk gone"))
        }
    }

    #[test]
    fn json_lines_sink_propagates_write_errors() {
        let exporter: Arc<dyn EventSink> = Arc::new(JsonLinesExporter::new(AlwaysFailsWriter));
        let events = vec![ExecutionEvent {
            wall_time: wall_clock_now(),
            offset_ms: 0,
            trace_id: "t".into(),
            span_id: "s".into(),
            parent_span_id: None,
            kind: EventKind::Metric,
            operation: "op".into(),
            status: EventStatus::Succeeded,
            duration_ms: None,
            metadata: BTreeMap::new(),
        }];
        let err = exporter.export(&events).unwrap_err();
        assert!(err.to_string().contains("disk gone"), "{err}");
    }

    #[test]
    fn json_lines_sink_fsync_file_round_trip() {
        let collector = EventCollector::new();
        let ctx = TraceContext::new();
        let span = collector.open_span(&ctx, EventKind::AgentStep, "step", None);
        span.finish(EventStatus::Succeeded);

        let dir = std::env::temp_dir();
        let path = dir.join(format!(
            "ai-sdk-fsync-{}-{}.jsonl",
            std::process::id(),
            ctx.trace_id()
        ));
        let sink: Arc<dyn EventSink> =
            Arc::new(JsonLinesExporter::create_file(&path).expect("file created"));
        let expected = collector.events();
        collector.try_flush(&[sink]).expect("durable export");

        let text = std::fs::read_to_string(&path).expect("file readable after fsync");
        std::fs::remove_file(&path).ok();
        let reloaded = EventCollector::new();
        for line in text.lines() {
            reloaded.insert_event(ExecutionEvent::from_jsonl(line).unwrap());
        }
        assert_eq!(reloaded.events(), expected);
    }

    #[test]
    fn drop_status_distinguishes_panics() {
        assert_eq!(EventGuard::drop_status(false), EventStatus::Succeeded);
        assert_eq!(EventGuard::drop_status(true), EventStatus::Failed);
    }

    #[test]
    fn guard_dropped_by_panic_records_failed() {
        let collector = EventCollector::new();
        let shared = collector.clone();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = EventGuard::new(
                &shared,
                EventKind::ToolCall,
                "exploding-tool",
                "trace-p".into(),
                "span-p".into(),
                None,
            );
            panic!("scope exploded");
        }));
        assert!(result.is_err(), "expected the panic to be caught");
        let events = collector.events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].status, EventStatus::Failed);
        assert_eq!(events[0].operation, "exploding-tool");
        assert!(events[0].duration_ms.is_some());
    }

    #[test]
    fn exporter_as_sink_adapts_legacy_exporters() {
        let collector = EventCollector::new();
        collector.record(
            EventKind::Completed,
            "adapted",
            EventStatus::Succeeded,
            BTreeMap::new(),
        );
        let legacy: Arc<dyn EventExporter> = Arc::new(JsonLinesExporter::new(Vec::new()));
        let sink: Arc<dyn EventSink> = Arc::new(ExporterAsSink::new(legacy));
        collector.try_flush(&[sink]).expect("adapter never fails");
        assert!(collector.is_empty());
    }
}
