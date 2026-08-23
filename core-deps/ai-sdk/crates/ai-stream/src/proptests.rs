//! Property-based tests for the SSE parser.
//!
//! The key streaming guarantee: parsing is lossless no matter how the
//! underlying byte stream is split into chunks — every chunking must yield
//! the exact same event sequence. This is exactly the class of bug the
//! parser had when an early return mid-chunk dropped the chunk's remaining
//! lines (fixed, with regression tests, before these properties were added).

#![cfg(test)]

use std::pin::Pin;

use futures::Stream;
use futures::StreamExt;
use proptest::prelude::*;

use ai_errors::AiError;

use crate::{SseEvent, sse_parse};

fn rt() -> &'static tokio::runtime::Runtime {
    static RT: std::sync::OnceLock<tokio::runtime::Runtime> = std::sync::OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("current-thread runtime builds")
    })
}

fn text_strategy(max_len: usize) -> impl Strategy<Value = String> {
    prop::collection::vec(any::<char>(), 0..max_len).prop_map(|chars| chars.into_iter().collect())
}

fn sse_event_strategy() -> impl Strategy<Value = SseEvent> {
    let id = prop::option::weighted(0.3, text_strategy(20))
        .prop_filter("id must be single-line", |s| {
            s.as_deref().is_none_or(|s| !s.contains(['\n', '\r']))
        });
    let event =
        text_strategy(12).prop_filter("event must be single-line", |s| !s.contains(['\n', '\r']));
    let data = text_strategy(80).prop_filter("data must not contain CR", |s| !s.contains('\r'));
    let retry = prop::option::weighted(0.2, any::<u64>());
    (id, event, data, retry).prop_map(|(id, event, data, retry)| SseEvent {
        id,
        event,
        data,
        retry,
    })
}

/// Serializes an event back to its wire representation (the inverse of the
/// parser's `feed_line`/`dispatch`): one field per line, blank line ends
/// the event, multi-line data becomes repeated `data:` lines.
///
/// Events whose `data` is empty are serialized WITHOUT any `data:` line:
/// per the SSE spec such events are never dispatched, and the expectation
/// below filters them out — the two sides must agree.
fn serialize_event(event: &SseEvent) -> String {
    let mut out = String::new();
    if let Some(id) = &event.id {
        out.push_str("id: ");
        out.push_str(id);
        out.push('\n');
    }
    out.push_str("event: ");
    out.push_str(&event.event);
    out.push('\n');
    if !event.data.is_empty() {
        for line in event.data.split('\n') {
            out.push_str("data: ");
            out.push_str(line);
            out.push('\n');
        }
    }
    if let Some(retry) = event.retry {
        out.push_str("retry: ");
        out.push_str(&retry.to_string());
        out.push('\n');
    }
    out.push('\n');
    out
}

/// Splits bytes at the given (arbitrary) offsets; empty chunks allowed.
fn chunk_bytes(bytes: &[u8], sizes: &[usize]) -> Vec<Vec<u8>> {
    let mut chunks = Vec::new();
    let mut start = 0;
    for size in sizes {
        let end = (start + *size).min(bytes.len());
        chunks.push(bytes[start..end].to_vec());
        start = end;
        if start == bytes.len() {
            return chunks;
        }
    }
    if start < bytes.len() {
        chunks.push(bytes[start..].to_vec());
    }
    chunks
}

fn parse_chunks(chunks: Vec<Vec<u8>>) -> Vec<SseEvent> {
    let stream: Pin<Box<dyn Stream<Item = Result<bytes::Bytes, AiError>> + Send>> = Box::pin(
        futures::stream::iter(chunks.into_iter().map(|c| Ok(bytes::Bytes::from(c)))),
    );
    let mut sse = sse_parse(stream);
    let mut out = Vec::new();
    while let Some(event) = rt().block_on(sse.next()) {
        out.push(event.expect("no errors are injected"));
    }
    out
}

proptest! {
    /// No matter how the serialized event stream is split into byte chunks
    /// (including empty chunks and single-byte chunks), the parser yields
    /// exactly the same events.
    #[test]
    fn chunk_splitting_is_lossless(
        events in prop::collection::vec(sse_event_strategy(), 0..8),
        sizes in prop::collection::vec(0usize..=24, 0..64),
    ) {
        let wire: String = events.iter().map(serialize_event).collect();
        let chunks = chunk_bytes(wire.as_bytes(), &sizes);
        let parsed = parse_chunks(chunks);
        // The parser only yields events that carried at least one `data:`
        // line (empty-data events are dropped by the spec).
        let expected: Vec<SseEvent> = events
            .iter()
            .filter(|e| !e.data.is_empty())
            .cloned()
            .collect();
        prop_assert_eq!(parsed, expected);
    }

    /// The parser never panics and always terminates on arbitrary bytes.
    #[test]
    fn arbitrary_bytes_never_panic(
        chunks in prop::collection::vec(
            prop::collection::vec(any::<u8>(), 0..128),
            0..16,
        ),
    ) {
        let parsed = parse_chunks(chunks);
        for event in &parsed {
            // The parser always assigns the default event type.
            prop_assert!(!event.event.is_empty());
        }
    }
}
