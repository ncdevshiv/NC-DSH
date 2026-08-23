//! Criterion benchmarks for `ai-stream`: SSE parsing over synthetic streams.
//!
//! Scenarios:
//! - `many_small_events` — a chunk-set of 1000 small events spread across
//!   network-sized chunks (the common provider-stream shape).
//! - `large_multiline_data` — few events, each carrying many large
//!   multi-line `data:` payloads.
//! - `pathological_1byte_chunks` — the whole payload delivered one byte per
//!   chunk, stressing the incremental (partial-line) buffering path.
//!
//! The parser API is async (`sse_parse` consumes a byte stream), so the
//! benches drive it through a multi-thread tokio runtime via criterion's
//! async support (`criterion` / `async_tokio`). A pure-sync path —
//! [`ai_stream::ToolCallAccumulator`] assembly — is also benched without
//! any runtime.

use std::time::Duration;

use bytes::Bytes;
use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use futures::StreamExt;
use futures::stream;

use ai_errors::AiError;
use ai_stream::{ToolCallAccumulator, sse_parse};

/// Builds SSE text for `n_events` small events:
/// `id:`/`event:`/`data:` fields plus the blank dispatch line.
fn render_small_events(n_events: usize) -> String {
    let mut out = String::with_capacity(n_events * 48);
    for i in 0..n_events {
        out.push_str("id: ");
        out.push_str(&i.to_string());
        out.push_str("\nevent: delta\ndata: event-");
        out.push_str(&i.to_string());
        out.push_str("\n\n");
    }
    out
}

/// Splits `text` into `chunks` roughly equal `Bytes` chunks (>= 1).
fn split_into_chunks(text: &str, chunks: usize) -> Vec<Bytes> {
    let bytes = text.as_bytes();
    let chunks = chunks.max(1);
    let per = bytes.len().div_ceil(chunks);
    bytes
        .chunks(per.max(1))
        .map(Bytes::copy_from_slice)
        .collect()
}

/// Drains an `sse_parse` stream built from `chunks`, returning event count.
async fn drain_sse(chunks: Vec<Bytes>) -> usize {
    let input = stream::iter(chunks.into_iter().map(Ok::<Bytes, AiError>));
    sse_parse(input)
        .map(|event| event.expect("bench stream must parse cleanly"))
        .count()
        .await
}

fn bench_many_small_events(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");

    // 1000 events per chunk-set, spread across 50 network-sized chunks.
    let payload = render_small_events(1000);
    let chunks = split_into_chunks(&payload, 50);
    let total_bytes: u64 = chunks.iter().map(|c| c.len() as u64).sum();
    assert_eq!(total_bytes as usize, payload.len());

    let mut group = c.benchmark_group("sse_parse/many_small_events");
    group.throughput(Throughput::Bytes(total_bytes));
    group.bench_function(BenchmarkId::from_parameter("1000_events_50_chunks"), |b| {
        b.iter(|| {
            let n = rt.block_on(drain_sse(chunks.clone()));
            assert_eq!(n, 1000, "all events must be emitted");
        })
    });
    group.finish();
}

fn bench_large_multiline_data(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");

    // 64 events; each has 32 data lines of 128 chars (multi-line join path).
    const EVENTS: usize = 64;
    const LINES_PER_EVENT: usize = 32;
    const LINE_LEN: usize = 128;
    let line: String = "x".repeat(LINE_LEN);
    let mut payload = String::new();
    for i in 0..EVENTS {
        payload.push_str(&format!("id: {i}\ndata: "));
        for l in 0..LINES_PER_EVENT {
            payload.push_str(&line);
            if l + 1 < LINES_PER_EVENT {
                payload.push('\n');
            }
        }
        payload.push_str("\n\n");
    }
    // One big chunk: exercises the scan loop over a large buffer.
    let chunks = vec![Bytes::from(payload.clone())];
    let total_bytes = payload.len() as u64;

    let mut group = c.benchmark_group("sse_parse/large_multiline_data");
    group.throughput(Throughput::Bytes(total_bytes));
    group.bench_function(BenchmarkId::from_parameter("64x32x128_single_chunk"), |b| {
        b.iter(|| {
            let n = rt.block_on(drain_sse(chunks.clone()));
            assert_eq!(n, EVENTS, "all events must be emitted");
        })
    });
    group.finish();
}

fn bench_pathological_1byte_chunks(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");

    // 200 small events delivered ONE BYTE PER CHUNK: every chunk lands in
    // the partial-line buffer and no line completes until a terminator
    // byte arrives.
    let payload = render_small_events(200);
    let chunks: Vec<Bytes> = payload
        .as_bytes()
        .iter()
        .map(|&b| Bytes::copy_from_slice(&[b]))
        .collect();
    let total_bytes = payload.len() as u64;

    let mut group = c.benchmark_group("sse_parse/pathological_1byte_chunks");
    group.throughput(Throughput::Bytes(total_bytes));
    group.bench_function(
        BenchmarkId::from_parameter("200_events_byte_per_chunk"),
        |b| {
            b.iter(|| {
                let n = rt.block_on(drain_sse(chunks.clone()));
                assert_eq!(n, 200, "all events must be emitted");
            })
        },
    );
    group.finish();
}

/// Pure-sync path: tool-call assembly (`ToolCallAccumulator`) with no async
/// runtime involved — one started event, N argument deltas, completed.
fn bench_tool_call_accumulator(c: &mut Criterion) {
    use ai_types::{StreamEvent, ToolCall};

    fn events_for_calls(n_calls: usize, deltas_per_call: usize) -> Vec<StreamEvent> {
        let mut events = Vec::with_capacity(n_calls * (deltas_per_call + 2));
        for i in 0..n_calls {
            events.push(StreamEvent::ToolCallStarted {
                id: format!("call-{i}"),
                name: "search".to_string(),
            });
            for d in 0..deltas_per_call {
                events.push(StreamEvent::ToolCallDelta {
                    id: format!("call-{i}"),
                    arguments_delta: format!(r#"{{"q":"{d}"}}"#),
                });
            }
            events.push(StreamEvent::ToolCallCompleted {
                call: ToolCall {
                    id: format!("call-{i}"),
                    name: "search".to_string(),
                    arguments: format!(
                        r#"{{"q":{}}}"#,
                        (0..deltas_per_call)
                            .map(|d| d.to_string())
                            .collect::<Vec<_>>()
                            .join(",")
                    ),
                },
            });
        }
        events
    }

    let mut group = c.benchmark_group("tool_call_accumulator/sync");
    for &(calls, deltas) in &[(8usize, 4usize), (64, 4)] {
        let events = events_for_calls(calls, deltas);
        group.throughput(Throughput::Elements(events.len() as u64));
        group.bench_function(
            BenchmarkId::new("push_finalize", format!("{calls}x{deltas}")),
            |b| {
                b.iter_batched(
                    ToolCallAccumulator::new,
                    |mut acc| {
                        for event in &events {
                            acc.push(event);
                        }
                        acc.finalize_and_drain()
                    },
                    criterion::BatchSize::SmallInput,
                )
            },
        );
    }
    group.finish();
}

fn benchmark_group(c: &mut Criterion) {
    // Keep default measurement settings; CI smoke runs use `--test`.
    bench_many_small_events(c);
    bench_large_multiline_data(c);
    bench_pathological_1byte_chunks(c);
    bench_tool_call_accumulator(c);
}

criterion_group! {
    name = benches;
    config = Criterion::default().warm_up_time(Duration::from_millis(500));
    targets = benchmark_group
}
criterion_main!(benches);
