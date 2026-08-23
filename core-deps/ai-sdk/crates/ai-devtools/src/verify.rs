//! Invariant validation for recorded traces (CHRONO phase 2).
//!
//! `ai trace verify` checks a JSONL recording against four invariants and
//! exits non-zero when any is violated:
//!
//! 1. **UnfinishedSpan** — every `Started` event has a terminal event
//!    (`Succeeded` / `Failed` / `Cancelled`) for the same
//!    `(trace_id, span_id)`.
//! 2. **NonMonotonicOffsets** — `offset_ms` never decreases between
//!    consecutive events of the same trace (in file order).
//! 3. **RootCount** — exactly one root span per trace id, where a root is
//!    an event with no `parent_span_id`; repeated events of the same root
//!    span count once.
//! 4. **InconsistentDuration** — for a span that has both a `Started`
//!    event and a terminal event carrying `duration_ms`, the terminal
//!    event's offset minus the start offset must equal the recorded
//!    duration (`end - start == duration` when both are present).
//!
//! All functions are pure; [`verify_events`] takes plain slices so tests
//! can feed crafted fixtures without files or I/O.

use std::collections::{BTreeMap, BTreeSet};

use ai_observability::{EventStatus, ExecutionEvent};
use serde::Serialize;

/// The invariant a violation refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ViolationRule {
    UnfinishedSpan,
    NonMonotonicOffsets,
    RootCount,
    InconsistentDuration,
}

impl ViolationRule {
    /// Stable short name used in both human and JSON output.
    pub fn name(self) -> &'static str {
        match self {
            ViolationRule::UnfinishedSpan => "unfinished_span",
            ViolationRule::NonMonotonicOffsets => "non_monotonic_offsets",
            ViolationRule::RootCount => "root_count",
            ViolationRule::InconsistentDuration => "inconsistent_duration",
        }
    }
}

/// One detected invariant violation.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Violation {
    pub rule: ViolationRule,
    pub trace_id: String,
    /// Span the violation concerns, when attributable to one.
    pub span_id: Option<String>,
    pub message: String,
}

/// Terminal statuses that close a started span. (`Retrying` keeps a span
/// open by definition.)
fn is_terminal(status: EventStatus) -> bool {
    matches!(
        status,
        EventStatus::Succeeded | EventStatus::Failed | EventStatus::Cancelled
    )
}

/// Validates all invariants over `events` and returns every violation found
/// (empty means the recording is valid). Events may span multiple traces;
/// checks are applied per trace id.
pub fn verify_events(events: &[ExecutionEvent]) -> Vec<Violation> {
    let mut violations = Vec::new();

    // Group per trace, preserving file order.
    let mut by_trace: BTreeMap<String, Vec<&ExecutionEvent>> = BTreeMap::new();
    for event in events {
        by_trace
            .entry(event.trace_id.clone())
            .or_default()
            .push(event);
    }

    for (trace_id, trace_events) in &by_trace {
        verify_started_has_terminal(trace_id, trace_events, &mut violations);
        verify_monotonic_offsets(trace_id, trace_events, &mut violations);
        verify_single_root(trace_id, trace_events, &mut violations);
        verify_durations(trace_id, trace_events, &mut violations);
    }

    violations
}

fn verify_started_has_terminal(
    trace_id: &str,
    events: &[&ExecutionEvent],
    violations: &mut Vec<Violation>,
) {
    let mut open: Vec<&ExecutionEvent> = Vec::new();
    let mut closed_spans: BTreeSet<String> = BTreeSet::new();
    for event in events {
        if is_terminal(event.status) {
            closed_spans.insert(event.span_id.clone());
        } else if event.status == EventStatus::Started {
            open.push(event);
        }
    }
    for started in open {
        if !closed_spans.contains(&started.span_id) {
            violations.push(Violation {
                rule: ViolationRule::UnfinishedSpan,
                trace_id: trace_id.to_string(),
                span_id: Some(started.span_id.clone()),
                message: format!(
                    "span `{}` started at {} ms has no terminal event",
                    started.span_id, started.offset_ms
                ),
            });
        }
    }
}

fn verify_monotonic_offsets(
    trace_id: &str,
    events: &[&ExecutionEvent],
    violations: &mut Vec<Violation>,
) {
    for pair in events.windows(2) {
        let (previous, current) = (pair[0], pair[1]);
        if current.offset_ms < previous.offset_ms {
            violations.push(Violation {
                rule: ViolationRule::NonMonotonicOffsets,
                trace_id: trace_id.to_string(),
                span_id: Some(current.span_id.clone()),
                message: format!(
                    "offset went backwards: {} ms after {} ms (span `{}`, op `{}`)",
                    current.offset_ms, previous.offset_ms, current.span_id, current.operation
                ),
            });
        }
    }
}

fn verify_single_root(trace_id: &str, events: &[&ExecutionEvent], violations: &mut Vec<Violation>) {
    // A root is an event with no parent span; repeated events of the same
    // root span count once, so compare distinct span ids.
    let mut roots: BTreeSet<&str> = BTreeSet::new();
    for event in events {
        if event.parent_span_id.is_none() {
            roots.insert(event.span_id.as_str());
        }
    }
    if roots.len() != 1 {
        let listed = roots.iter().copied().collect::<Vec<_>>().join(", ");
        violations.push(Violation {
            rule: ViolationRule::RootCount,
            trace_id: trace_id.to_string(),
            span_id: None,
            message: format!(
                "expected exactly 1 root span, found {}: {listed}",
                roots.len()
            ),
        });
    }
}

fn verify_durations(trace_id: &str, events: &[&ExecutionEvent], violations: &mut Vec<Violation>) {
    // Collect start offsets per span.
    let mut starts: BTreeMap<String, u64> = BTreeMap::new();
    for event in events {
        if event.status == EventStatus::Started {
            starts
                .entry(event.span_id.clone())
                .or_insert(event.offset_ms);
        }
    }
    for event in events {
        let (Some(start_offset), Some(duration)) =
            (starts.get(&event.span_id).copied(), event.duration_ms)
        else {
            continue;
        };
        if !is_terminal(event.status) {
            continue;
        }
        let end_minus_start = event.offset_ms.saturating_sub(start_offset);
        if end_minus_start != duration {
            violations.push(Violation {
                rule: ViolationRule::InconsistentDuration,
                trace_id: trace_id.to_string(),
                span_id: Some(event.span_id.clone()),
                message: format!(
                    "duration {} ms does not match end-start {} ms (start at {} ms, end at {} ms)",
                    duration, end_minus_start, start_offset, event.offset_ms
                ),
            });
        }
    }
}

/// Renders violations as a human-readable list, one line each.
pub fn format_violations(violations: &[Violation]) -> String {
    let mut out = String::new();
    for (index, violation) in violations.iter().enumerate() {
        out.push_str(&format!(
            "{}. [{}] trace={}{}: {}\n",
            index + 1,
            violation.rule.name(),
            violation.trace_id,
            violation
                .span_id
                .as_ref()
                .map(|s| format!(" span={s}"))
                .unwrap_or_default(),
            violation.message
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use ai_observability::EventKind;
    use std::collections::BTreeMap;

    // Fixture constructor mirrors ExecutionEvent's full field set; the
    // explicit call sites read better than a parameter struct here.
    #[allow(clippy::too_many_arguments)]
    fn event(
        trace_id: &str,
        span_id: &str,
        parent: Option<&str>,
        kind: EventKind,
        operation: &str,
        status: EventStatus,
        offset_ms: u64,
        duration_ms: Option<u64>,
    ) -> ExecutionEvent {
        ExecutionEvent {
            wall_time: "2025-01-01T00:00:00Z".to_string(),
            offset_ms,
            trace_id: trace_id.to_string(),
            span_id: span_id.to_string(),
            parent_span_id: parent.map(str::to_string),
            kind,
            operation: operation.to_string(),
            status,
            duration_ms,
            metadata: BTreeMap::new(),
        }
    }

    /// A fully valid two-trace recording with a multi-span tree:
    /// trace `t1`: root span s1 (Started -> Completed, 0..12 ms)
    ///             child span s2 under s1 (single terminal event).
    /// trace `t2`: one standalone completed metric span.
    fn valid_two_trace_fixture() -> Vec<ExecutionEvent> {
        vec![
            event(
                "t1",
                "s1",
                None,
                EventKind::RequestStarted,
                "request",
                EventStatus::Started,
                0,
                None,
            ),
            event(
                "t1",
                "s2",
                Some("s1"),
                EventKind::AgentStep,
                "step",
                EventStatus::Succeeded,
                5,
                Some(10),
            ),
            event(
                "t1",
                "s1",
                Some("s1"),
                EventKind::Completed,
                "request",
                EventStatus::Succeeded,
                12,
                Some(12),
            ),
            event(
                "t2",
                "m1",
                None,
                EventKind::Metric,
                "tokens",
                EventStatus::Succeeded,
                20,
                Some(3),
            ),
        ]
    }

    #[test]
    fn valid_fixture_passes_with_no_violations() {
        assert_eq!(verify_events(&valid_two_trace_fixture()), vec![]);
    }

    #[test]
    fn unfinished_span_is_detected() {
        let events = vec![
            event(
                "tA",
                "s1",
                None,
                EventKind::RequestStarted,
                "request",
                EventStatus::Started,
                0,
                None,
            ),
            event(
                "tA",
                "s9",
                Some("s1"),
                EventKind::Metric,
                "other-span",
                EventStatus::Succeeded,
                3,
                Some(1),
            ),
        ];
        let violations = verify_events(&events);
        assert_eq!(violations.len(), 1, "{violations:?}");
        assert_eq!(violations[0].rule, ViolationRule::UnfinishedSpan);
        assert_eq!(violations[0].span_id.as_deref(), Some("s1"));
    }

    #[test]
    fn retrying_does_not_close_a_span() {
        let events = vec![
            event(
                "tR",
                "s1",
                None,
                EventKind::ModelCall,
                "call",
                EventStatus::Started,
                0,
                None,
            ),
            event(
                "tR",
                "s1",
                Some("s1"),
                EventKind::Retry,
                "backoff",
                EventStatus::Retrying,
                2,
                None,
            ),
        ];
        let violations = verify_events(&events);
        assert!(
            violations
                .iter()
                .any(|v| v.rule == ViolationRule::UnfinishedSpan),
            "{violations:?}"
        );
    }

    #[test]
    fn decreasing_offsets_are_detected() {
        let events = vec![
            event(
                "tB",
                "s1",
                None,
                EventKind::Metric,
                "first",
                EventStatus::Succeeded,
                10,
                None,
            ),
            event(
                "tB",
                "s2",
                Some("s1"),
                EventKind::Metric,
                "second",
                EventStatus::Succeeded,
                5,
                None,
            ),
        ];
        let violations = verify_events(&events);
        assert_eq!(violations.len(), 1, "{violations:?}");
        assert_eq!(violations[0].rule, ViolationRule::NonMonotonicOffsets);
        assert!(violations[0].message.contains("10 ms"));
    }

    #[test]
    fn equal_offsets_are_monotonic() {
        let events = vec![
            event(
                "tE",
                "s1",
                None,
                EventKind::Metric,
                "a",
                EventStatus::Succeeded,
                7,
                None,
            ),
            event(
                "tE",
                "s2",
                Some("s1"),
                EventKind::Metric,
                "b",
                EventStatus::Succeeded,
                7,
                None,
            ),
        ];
        assert_eq!(verify_events(&events), vec![]);
    }

    #[test]
    fn multiple_roots_are_detected() {
        let events = vec![
            event(
                "tC",
                "r1",
                None,
                EventKind::RequestStarted,
                "one",
                EventStatus::Started,
                0,
                None,
            ),
            event(
                "tC",
                "r2",
                None,
                EventKind::RequestStarted,
                "two",
                EventStatus::Started,
                1,
                None,
            ),
            event(
                "tC",
                "r1",
                None,
                EventKind::Completed,
                "one",
                EventStatus::Succeeded,
                5,
                Some(5),
            ),
            event(
                "tC",
                "r2",
                None,
                EventKind::Completed,
                "two",
                EventStatus::Succeeded,
                7,
                Some(6),
            ),
        ];
        let violations = verify_events(&events);
        let root_violations: Vec<_> = violations
            .iter()
            .filter(|v| v.rule == ViolationRule::RootCount)
            .collect();
        assert_eq!(root_violations.len(), 1, "{violations:?}");
        assert!(root_violations[0].message.contains("found 2"));
    }

    #[test]
    fn zero_roots_is_a_root_count_violation() {
        let events = vec![event(
            "tD",
            "child",
            Some("ghost"),
            EventKind::Metric,
            "m",
            EventStatus::Succeeded,
            0,
            None,
        )];
        let violations = verify_events(&events);
        assert!(
            violations
                .iter()
                .any(|v| v.rule == ViolationRule::RootCount && v.message.contains("found 0"))
        );
    }

    #[test]
    fn repeated_root_events_count_once() {
        // The same root span emits Started + Completed, both parentless:
        // still exactly ONE root.
        let events = vec![
            event(
                "tF",
                "r",
                None,
                EventKind::RequestStarted,
                "req",
                EventStatus::Started,
                0,
                None,
            ),
            event(
                "tF",
                "r",
                None,
                EventKind::Completed,
                "req",
                EventStatus::Succeeded,
                9,
                Some(9),
            ),
        ];
        assert_eq!(verify_events(&events), vec![]);
    }

    #[test]
    fn inconsistent_duration_is_detected() {
        let events = vec![
            event(
                "tG",
                "s1",
                None,
                EventKind::ModelCall,
                "call",
                EventStatus::Started,
                0,
                None,
            ),
            event(
                "tG",
                "s1",
                Some("s1"),
                EventKind::ModelCall,
                "call",
                EventStatus::Succeeded,
                10,
                Some(99),
            ),
        ];
        let violations = verify_events(&events);
        assert_eq!(violations.len(), 1, "{violations:?}");
        assert_eq!(violations[0].rule, ViolationRule::InconsistentDuration);
        assert!(violations[0].message.contains("99 ms"));
    }

    #[test]
    fn consistent_duration_passes() {
        let events = vec![
            event(
                "tH",
                "s1",
                None,
                EventKind::ModelCall,
                "call",
                EventStatus::Started,
                40,
                None,
            ),
            event(
                "tH",
                "s1",
                Some("s1"),
                EventKind::ModelCall,
                "call",
                EventStatus::Failed,
                53,
                Some(13),
            ),
        ];
        assert_eq!(verify_events(&events), vec![]);
    }

    #[test]
    fn duration_without_matching_start_is_not_checked() {
        // A lone terminal event with a duration has no start to compare
        // against; the invariant only applies when both ends are present.
        let events = vec![event(
            "tI",
            "only",
            None,
            EventKind::ToolCall,
            "tool",
            EventStatus::Succeeded,
            100,
            Some(42),
        )];
        assert_eq!(verify_events(&events), vec![]);
    }

    #[test]
    fn multiple_traces_report_per_trace() {
        let mut events = valid_two_trace_fixture();
        events.push(event(
            "tX",
            "orphan",
            None,
            EventKind::AgentStep,
            "lost",
            EventStatus::Started,
            30,
            None,
        ));
        let violations = verify_events(&events);
        assert_eq!(violations.len(), 1, "{violations:?}");
        assert_eq!(violations[0].trace_id, "tX");
    }

    #[test]
    fn human_format_lists_rule_names_and_indexes() {
        let events = vec![event(
            "tJ",
            "s1",
            None,
            EventKind::RequestStarted,
            "request",
            EventStatus::Started,
            0,
            None,
        )];
        let text = format_violations(&verify_events(&events));
        assert_eq!(
            text,
            "1. [unfinished_span] trace=tJ span=s1: span `s1` started at 0 ms has no terminal event\n"
        );
    }

    #[test]
    fn violations_serialize_to_json() {
        let events = vec![event(
            "tK",
            "s1",
            None,
            EventKind::RequestStarted,
            "request",
            EventStatus::Started,
            0,
            None,
        )];
        let json = serde_json::to_string(&verify_events(&events)).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed[0]["rule"], "unfinished_span");
        assert_eq!(parsed[0]["trace_id"], "tK");
    }
}
