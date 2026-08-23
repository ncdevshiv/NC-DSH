//! Unified streaming: SSE parsing and stream aggregation helpers.
//!
//! Providers stream `text/event-stream` responses with divergent wire
//! formats. This crate provides:
//!
//! - [`sse_parse`] — a correct, incremental parser for the
//!   [Server-Sent Events](https://html.spec.whatwg.org/multipage/server-sent-events.html)
//!   format (field parsing, multi-line `data:`, `id:`, `event:`, `retry:`).
//! - [`collect_text`] / [`collect_completion`] — aggregate a unified
//!   [`StreamEvent`] stream into a `String` or a [`Completion`].
//! - [`ToolCallAccumulator`] — assembles `ToolCallStarted`/`Delta`/
//!   `Completed` events into complete tool calls (provider adapters reuse
//!   this instead of reimplementing assembly).

use std::pin::Pin;

use futures::Stream;
use futures::StreamExt;

use ai_core::EventStream;
use ai_errors::{AiError, SerializationError};
use ai_types::{Completion, ModelId, ProviderId, StreamEvent, ToolCall, Usage};

/// A parsed Server-Sent Event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SseEvent {
    /// `id:` field (optional).
    pub id: Option<String>,
    /// `event:` field (optional; default `message`).
    pub event: String,
    /// `data:` field with multi-line data joined by `\n`.
    pub data: String,
    /// `retry:` field in milliseconds (optional).
    pub retry: Option<u64>,
}

impl SseEvent {
    pub fn new(data: impl Into<String>) -> Self {
        Self {
            id: None,
            event: "message".to_string(),
            data: data.into(),
            retry: None,
        }
    }
}

/// Returns the byte length of the first line including its terminator, or
/// `None` if no line terminator is present in `bytes`.
///
/// `\r\n`, `\n`, and `\r` are all treated as a single terminator.
fn find_line_end(bytes: &[u8]) -> Option<usize> {
    bytes
        .iter()
        .position(|&b| b == b'\n' || b == b'\r')
        .map(|i| {
            if bytes[i] == b'\r' && bytes.get(i + 1) == Some(&b'\n') {
                i + 2
            } else {
                i + 1
            }
        })
}

/// Strips a trailing `\n`, `\r\n`, or `\r` from a line.
fn strip_line_end(line: &[u8]) -> &[u8] {
    if line.last() == Some(&b'\n') {
        let end = line.len() - 1;
        if end > 0 && line[end - 1] == b'\r' {
            &line[..end - 1]
        } else {
            &line[..end]
        }
    } else if line.last() == Some(&b'\r') {
        &line[..line.len() - 1]
    } else {
        line
    }
}

/// Parser state for [`parse_sse`].
struct SseParserState {
    input: Pin<Box<dyn Stream<Item = Result<bytes::Bytes, AiError>> + Send>>,
    buffer: Vec<u8>,
    /// Accumulated fields of the event currently being parsed.
    id: Option<String>,
    event: Option<String>,
    data_lines: Vec<String>,
    retry: Option<u64>,
    events: std::collections::VecDeque<SseEvent>,
    done: bool,
}

impl SseParserState {
    /// Creates parser state over the given byte stream.
    fn new<E>(input: impl Stream<Item = Result<bytes::Bytes, E>> + Send + 'static) -> Self
    where
        E: Into<AiError> + Send + 'static,
    {
        let mapped = input.map(|item| item.map_err(Into::into));
        Self {
            input: Box::pin(mapped),
            buffer: Vec::with_capacity(4096),
            id: None,
            event: None,
            data_lines: Vec::new(),
            retry: None,
            events: std::collections::VecDeque::new(),
            done: false,
        }
    }

    /// Processes one logical line (without line terminator).
    fn feed_line(&mut self, line: &[u8]) {
        if line.is_empty() {
            // Blank line: dispatch the accumulated event.
            self.dispatch();
            return;
        }
        if line.starts_with(b":") {
            // Comment line — ignore.
            return;
        }
        let (field, value) = match line.iter().position(|&b| b == b':') {
            Some(pos) => {
                let field = &line[..pos];
                let mut value = &line[pos + 1..];
                if value.first() == Some(&b' ') {
                    value = &value[1..];
                }
                (field, value)
            }
            None => (line, &[][..]),
        };
        match field {
            b"id" => self.id = Some(String::from_utf8_lossy(value).into_owned()),
            b"event" => self.event = Some(String::from_utf8_lossy(value).into_owned()),
            b"data" => self
                .data_lines
                .push(String::from_utf8_lossy(value).into_owned()),
            b"retry" => {
                self.retry = String::from_utf8_lossy(value).trim().parse().ok();
            }
            _ => { /* unknown fields are ignored per spec */ }
        }
    }

    /// Completes the current event and pushes it onto the queue (if it has
    /// data), resetting field accumulation.
    fn dispatch(&mut self) {
        if !self.data_lines.is_empty() {
            let event = SseEvent {
                id: self.id.take(),
                event: self.event.take().unwrap_or_else(|| "message".to_string()),
                data: self.data_lines.join("\n"),
                retry: self.retry.take(),
            };
            self.data_lines.clear();
            self.events.push_back(event);
        } else {
            // Reset fields even when there was no data.
            self.id = None;
            self.event = None;
            self.retry = None;
        }
    }
}

/// A parsed SSE event stream.
pub type SseStream = Pin<Box<dyn Stream<Item = Result<SseEvent, AiError>> + Send>>;

/// Constructs the parser with the given input stream.
pub fn sse_parse<E>(
    input: impl Stream<Item = Result<bytes::Bytes, E>> + Send + 'static,
) -> SseStream
where
    E: Into<AiError> + Send + 'static,
{
    let state = SseParserState::new(input);
    let stream = futures::stream::unfold(Some(state), |state: Option<SseParserState>| async move {
        let mut s = state.expect("unfold state is always present");
        loop {
            if let Some(event) = s.events.pop_front() {
                return Some((Ok(event), Some(s)));
            }
            if s.done {
                return None;
            }
            match s.input.next().await {
                Some(Ok(chunk)) => {
                    // Fold any partial line from the previous chunk together
                    // with this chunk, then process ALL complete lines of
                    // this chunk (queuing events) before returning. `combined`
                    // is an owned buffer so no borrow of `s` is held across
                    // the mutating calls below.
                    //
                    // Important: we must NOT return mid-chunk — an early
                    // return would lose the chunk's remaining lines, since
                    // the scan position (`start`) lives only in this
                    // invocation.
                    let mut combined = std::mem::take(&mut s.buffer);
                    combined.extend_from_slice(&chunk);
                    let mut start = 0;
                    loop {
                        match find_line_end(&combined[start..]) {
                            Some(len) => {
                                let line = strip_line_end(&combined[start..start + len]);
                                start += len;
                                if line.is_empty() {
                                    s.dispatch();
                                } else {
                                    s.feed_line(line);
                                }
                            }
                            None => {
                                s.buffer = combined[start..].to_vec();
                                break;
                            }
                        }
                    }
                }
                Some(Err(e)) => return Some((Err(e), Some(s))),
                None => {
                    // EOF: flush any trailing partial line, then dispatch the
                    // final event (which may lack a trailing blank line).
                    if !s.buffer.is_empty() {
                        let pending = std::mem::take(&mut s.buffer);
                        s.feed_line(&pending);
                    }
                    s.dispatch();
                    s.done = true;
                    if !s.events.is_empty() {
                        return Some((Ok(s.events.pop_front().unwrap()), Some(s)));
                    }
                    return None;
                }
            }
        }
    });
    Box::pin(stream)
}

/// Collects all text deltas from a unified event stream into a `String`.
///
/// The stream is consumed until completion; errors are propagated.
pub async fn collect_text(stream: EventStream) -> Result<String, AiError> {
    let mut stream = stream;
    let mut text = String::new();
    while let Some(event) = stream.next().await {
        match event? {
            StreamEvent::TextDelta { delta } => text.push_str(&delta),
            StreamEvent::Error { message } => {
                return Err(AiError::Serialization(SerializationError::new(format!(
                    "provider stream error: {message}"
                ))));
            }
            _ => {}
        }
    }
    Ok(text)
}

/// Assembles tool-call events into complete calls.
///
/// Keeps per-call state: `ToolCallStarted` records the name; `ToolCallDelta`
/// appends argument fragments; `ToolCallCompleted` finalizes the call.
#[derive(Debug, Default)]
pub struct ToolCallAccumulator {
    in_flight: std::collections::HashMap<String, (String, String)>,
    completed: Vec<ToolCall>,
}

impl ToolCallAccumulator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, event: &StreamEvent) {
        match event {
            StreamEvent::ToolCallStarted { id, name } => {
                self.in_flight
                    .insert(id.clone(), (name.clone(), String::new()));
            }
            StreamEvent::ToolCallDelta {
                id,
                arguments_delta,
            } => {
                if let Some((_name, args)) = self.in_flight.get_mut(id) {
                    args.push_str(arguments_delta);
                }
            }
            StreamEvent::ToolCallCompleted { call } => {
                self.in_flight.remove(&call.id);
                self.completed.push(call.clone());
            }
            _ => {}
        }
    }

    /// Completed calls, in order of completion.
    pub fn completed(&self) -> &[ToolCall] {
        &self.completed
    }

    /// Finalizes any in-flight calls (those that received `Started`/`Delta`
    /// but no `Completed`), using the accumulated arguments. Called by
    /// stream adapters when the provider ends a stream with `finish_reason`
    /// instead of explicit completion events.
    pub fn finalize(&mut self) {
        let pending: Vec<ToolCall> = self
            .in_flight
            .drain()
            .map(|(id, (name, arguments))| ToolCall {
                id,
                name,
                arguments,
            })
            .collect();
        for call in pending {
            self.completed.push(call);
        }
    }

    /// Finalizes in-flight calls and returns all completed calls, clearing
    /// the accumulator (one-shot emission for stream adapters).
    pub fn finalize_and_drain(&mut self) -> Vec<ToolCall> {
        self.finalize();
        self.drain_completed()
    }

    /// Removes and returns all completed calls (used by adapters that emit
    /// completion events exactly once).
    pub fn drain_completed(&mut self) -> Vec<ToolCall> {
        std::mem::take(&mut self.completed)
    }
}

/// Aggregates a unified event stream into a [`Completion`].
///
/// Text deltas are concatenated, tool calls are assembled via
/// [`ToolCallAccumulator`], and the last [`StreamEvent::UsageUpdate`]
/// provides token usage.
pub async fn collect_completion(
    provider: &ProviderId,
    model: &ModelId,
    stream: EventStream,
) -> Result<Completion, AiError> {
    let mut stream = stream;
    let mut text = String::new();
    let mut usage = Usage::default();
    let mut finish_reason = None;
    let mut tools = ToolCallAccumulator::new();

    while let Some(event) = stream.next().await {
        let event = event?;
        match &event {
            StreamEvent::TextDelta { delta } => text.push_str(delta),
            StreamEvent::UsageUpdate { usage: u } => usage = *u,
            StreamEvent::Completed { finish_reason: fr } => finish_reason = fr.clone(),
            StreamEvent::Error { message } => {
                return Err(AiError::Serialization(SerializationError::new(format!(
                    "provider stream error: {message}"
                ))));
            }
            _ => {}
        }
        tools.push(&event);
    }

    Ok(Completion {
        provider: provider.clone(),
        model: model.clone(),
        text,
        tool_calls: tools.completed().to_vec(),
        usage,
        reasoning: None,
        raw: serde_json::Value::Null,
        finish_reason,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use futures::stream;

    fn bytes_stream(
        chunks: Vec<&'static str>,
    ) -> impl Stream<Item = Result<Bytes, AiError>> + Send + 'static {
        stream::iter(chunks.into_iter().map(|c| Ok(Bytes::from(c.to_string()))))
    }

    #[tokio::test]
    async fn parses_single_event() {
        let input = bytes_stream(vec!["data: hello\n\n"]);
        let events: Vec<SseEvent> = sse_parse(input).map(|e| e.unwrap()).collect().await;
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "hello");
        assert_eq!(events[0].event, "message");
    }

    #[tokio::test]
    async fn handles_crlf_and_multiline_data() {
        let input = bytes_stream(vec!["data: line1\r\ndata: line2\r\n\r\n"]);
        let events: Vec<SseEvent> = sse_parse(input).map(|e| e.unwrap()).collect().await;
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "line1\nline2");
    }

    #[tokio::test]
    async fn handles_fields_and_comments() {
        let input = bytes_stream(vec![
            ": this is a comment\nid: 42\nevent: delta\ndata: chunk\n\n",
        ]);
        let events: Vec<SseEvent> = sse_parse(input).map(|e| e.unwrap()).collect().await;
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].id.as_deref(), Some("42"));
        assert_eq!(events[0].event, "delta");
        assert_eq!(events[0].data, "chunk");
    }

    #[tokio::test]
    async fn handles_chunk_boundaries() {
        // The event is split across chunks in awkward places.
        let input = bytes_stream(vec!["da", "ta: hel", "lo\n\n", "data: wor", "ld\n\n"]);
        let events: Vec<SseEvent> = sse_parse(input).map(|e| e.unwrap()).collect().await;
        let datas: Vec<String> = events.iter().map(|e| e.data.clone()).collect();
        assert_eq!(datas, vec!["hello", "world"]);
    }

    #[tokio::test]
    async fn flushes_final_event_without_trailing_blank_line() {
        let input = bytes_stream(vec!["data: last\n"]);
        let events: Vec<SseEvent> = sse_parse(input).map(|e| e.unwrap()).collect().await;
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "last");
    }

    #[tokio::test]
    async fn multiple_events_in_one_chunk_are_all_emitted() {
        // Regression test: a single network chunk containing several SSE
        // events must emit every event. An early-return bug previously
        // dropped all events after the first one in such chunks.
        let input = bytes_stream(vec![
            "data: one\n\ndata: two\n\ndata: three\n\ndata: four\n\n",
        ]);
        let events: Vec<SseEvent> = sse_parse(input).map(|e| e.unwrap()).collect().await;
        let datas: Vec<String> = events.iter().map(|e| e.data.clone()).collect();
        assert_eq!(datas, vec!["one", "two", "three", "four"]);
    }

    #[tokio::test]
    async fn many_events_across_variable_chunks_are_all_emitted() {
        // Regression: realistic network interleaving — big chunks holding
        // several events plus fragmented boundaries.
        let input = bytes_stream(vec![
            "data: a\n\ndata: b\n\ndata: c",
            "\n\ndata: d\n\n",
            "data: e",
            "\n\ndata: f\n\n",
        ]);
        let events: Vec<SseEvent> = sse_parse(input).map(|e| e.unwrap()).collect().await;
        let datas: Vec<String> = events.iter().map(|e| e.data.clone()).collect();
        assert_eq!(datas, vec!["a", "b", "c", "d", "e", "f"]);
    }

    #[tokio::test]
    async fn skips_events_without_data() {
        let input = bytes_stream(vec!["event: ping\n\n", "data: real\n\n"]);
        let events: Vec<SseEvent> = sse_parse(input).map(|e| e.unwrap()).collect().await;
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "real");
    }

    #[tokio::test]
    async fn propagates_stream_errors() {
        let input = stream::iter(vec![
            Ok(Bytes::from("data: ok\n\n")),
            Err(AiError::Internal(ai_errors::InternalError::new("boom"))),
        ]);
        let mut events = sse_parse(input);
        assert!(events.next().await.unwrap().is_ok());
        assert!(events.next().await.unwrap().is_err());
    }

    #[tokio::test]
    async fn collect_text_joins_deltas() {
        let events = stream::iter(vec![
            Ok(StreamEvent::TextDelta {
                delta: "Hel".into(),
            }),
            Ok(StreamEvent::TextDelta { delta: "lo".into() }),
            Ok(StreamEvent::Completed {
                finish_reason: None,
            }),
        ]);
        let text = collect_text(Box::pin(events)).await.unwrap();
        assert_eq!(text, "Hello");
    }

    #[tokio::test]
    async fn tool_call_accumulator_assembles_calls() {
        let mut acc = ToolCallAccumulator::new();
        acc.push(&StreamEvent::ToolCallStarted {
            id: "c1".into(),
            name: "calc".into(),
        });
        acc.push(&StreamEvent::ToolCallDelta {
            id: "c1".into(),
            arguments_delta: r#"{"expr""#.into(),
        });
        acc.push(&StreamEvent::ToolCallDelta {
            id: "c1".into(),
            arguments_delta: r#": "2+2"}"#.into(),
        });
        acc.push(&StreamEvent::ToolCallCompleted {
            call: ToolCall {
                id: "c1".into(),
                name: "calc".into(),
                arguments: r#"{"expr": "2+2"}"#.into(),
            },
        });
        assert_eq!(acc.completed().len(), 1);
        assert_eq!(acc.completed()[0].name, "calc");
        assert_eq!(acc.completed()[0].arguments, r#"{"expr": "2+2"}"#);
    }

    #[tokio::test]
    async fn collect_completion_aggregates_everything() {
        let events = stream::iter(vec![
            Ok(StreamEvent::TextDelta {
                delta: "Result: ".into(),
            }),
            Ok(StreamEvent::ToolCallStarted {
                id: "c1".into(),
                name: "calc".into(),
            }),
            Ok(StreamEvent::ToolCallDelta {
                id: "c1".into(),
                arguments_delta: "{}".into(),
            }),
            Ok(StreamEvent::ToolCallCompleted {
                call: ToolCall {
                    id: "c1".into(),
                    name: "calc".into(),
                    arguments: "{}".into(),
                },
            }),
            Ok(StreamEvent::UsageUpdate {
                usage: Usage::new(10, 5),
            }),
            Ok(StreamEvent::Completed {
                finish_reason: Some("tool_calls".into()),
            }),
        ]);
        let completion = collect_completion(
            &ProviderId::new("openai"),
            &ModelId::new("gpt-4o"),
            Box::pin(events),
        )
        .await
        .unwrap();
        assert_eq!(completion.text, "Result: ");
        assert_eq!(completion.tool_calls.len(), 1);
        assert_eq!(completion.usage.total(), 15);
        assert_eq!(completion.finish_reason.as_deref(), Some("tool_calls"));
    }
}

#[cfg(test)]
mod proptests;
