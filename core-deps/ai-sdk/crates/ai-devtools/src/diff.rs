//! Structural diff of two recorded runs (CHRONO phase 2).
//!
//! Recordings are compared **per trace id**. Events on each side are matched
//! by their `(span_id, operation)` key (duplicates pair up in order of
//! appearance), so a span that kept its identity but changed timing or
//! status is *matched*, while genuinely new/removed spans surface as
//! added/removed events. For every matched pair the comparison reports:
//!
//! - duration deltas greater than [`DURATION_DELTA_THRESHOLD_PERCENT`]
//!   (relative to the baseline recording),
//! - status changes,
//! - a per-trace finish-status mismatch summary (derived final status — any
//!   failure ⇒ `Failed`, otherwise `Succeeded`).
//!
//! All functions are pure and fixture-testable; rendering is separate.

use std::collections::{BTreeMap, BTreeSet};

use ai_observability::{EventStatus, ExecutionEvent};
use serde::Serialize;

/// Relative duration change (vs. the baseline side) above which a matched
/// span is reported as a duration delta.
pub const DURATION_DELTA_THRESHOLD_PERCENT: f64 = 20.0;

/// Where an unmatched event was observed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Side {
    /// Present only in the baseline (`a`) recording.
    #[serde(rename = "only-in-a")]
    A,
    /// Present only in the compared (`b`) recording.
    #[serde(rename = "only-in-b")]
    B,
}

impl Side {
    fn label(self) -> &'static str {
        match self {
            Side::A => "only-in-a",
            Side::B => "only-in-b",
        }
    }
}

/// An event that exists on one side of the diff only.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UnmatchedEvent {
    pub side: Side,
    pub trace_id: String,
    pub span_id: String,
    pub operation: String,
    pub offset_ms: u64,
}

/// A matched span whose duration changed by more than
/// [`DURATION_DELTA_THRESHOLD_PERCENT`] relative to the baseline.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DurationDelta {
    pub trace_id: String,
    pub span_id: String,
    pub operation: String,
    pub baseline_ms: u64,
    pub compared_ms: u64,
    /// Signed percentage change relative to `baseline_ms`.
    pub percent: f64,
}

/// A matched span whose lifecycle status changed between recordings.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StatusChange {
    pub trace_id: String,
    pub span_id: String,
    pub operation: String,
    pub baseline: EventStatus,
    pub compared: EventStatus,
}

/// A trace whose derived final status differs between recordings.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FinishMismatch {
    pub trace_id: String,
    pub baseline_finish: EventStatus,
    pub compared_finish: EventStatus,
}

/// Everything that differs between two recordings.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct RecordingDiff {
    pub traces_compared: usize,
    pub events_added: Vec<UnmatchedEvent>,
    pub events_removed: Vec<UnmatchedEvent>,
    pub duration_deltas: Vec<DurationDelta>,
    pub status_changes: Vec<StatusChange>,
    pub finish_mismatches: Vec<FinishMismatch>,
}

impl RecordingDiff {
    /// True when the two recordings are structurally identical.
    pub fn is_empty(&self) -> bool {
        self.events_added.is_empty()
            && self.events_removed.is_empty()
            && self.duration_deltas.is_empty()
            && self.status_changes.is_empty()
            && self.finish_mismatches.is_empty()
    }

    /// Total number of reported differences.
    pub fn difference_count(&self) -> usize {
        self.events_added.len()
            + self.events_removed.len()
            + self.duration_deltas.len()
            + self.status_changes.len()
            + self.finish_mismatches.len()
    }
}

/// Derives a trace's final status: `Failed` if any event failed, otherwise
/// `Succeeded`. Mirrors the aggregate rule used by trace list views.
pub fn finish_status(events: &[ExecutionEvent]) -> EventStatus {
    if events.iter().any(|e| e.status == EventStatus::Failed) {
        EventStatus::Failed
    } else {
        EventStatus::Succeeded
    }
}

fn key(event: &ExecutionEvent) -> (String, String) {
    (event.span_id.clone(), event.operation.clone())
}

/// Groups events by `(span_id, operation)`, preserving per-key file order so
/// duplicate keys pair up positionally between recordings.
fn group_by_key(events: &[ExecutionEvent]) -> BTreeMap<(String, String), Vec<&ExecutionEvent>> {
    let mut grouped: BTreeMap<(String, String), Vec<&ExecutionEvent>> = BTreeMap::new();
    for event in events {
        grouped.entry(key(event)).or_default().push(event);
    }
    grouped
}

fn split_by_trace(events: &[ExecutionEvent]) -> BTreeMap<String, Vec<ExecutionEvent>> {
    let mut by_trace: BTreeMap<String, Vec<ExecutionEvent>> = BTreeMap::new();
    for event in events {
        by_trace
            .entry(event.trace_id.clone())
            .or_default()
            .push(event.clone());
    }
    by_trace
}

/// Structurally diffs recording `baseline` against recording `compared`.
///
/// Both slices may contain multiple traces; comparison happens per trace id.
pub fn diff_recordings(baseline: &[ExecutionEvent], compared: &[ExecutionEvent]) -> RecordingDiff {
    let mut by_trace_baseline = split_by_trace(baseline);
    let mut by_trace_compared = split_by_trace(compared);

    let mut diff = RecordingDiff {
        traces_compared: by_trace_baseline.len().max(by_trace_compared.len()),
        ..RecordingDiff::default()
    };

    let trace_ids: BTreeSet<String> = by_trace_baseline
        .keys()
        .chain(by_trace_compared.keys())
        .cloned()
        .collect();

    for trace_id in trace_ids {
        let base_events = by_trace_baseline.remove(&trace_id).unwrap_or_default();
        let comp_events = by_trace_compared.remove(&trace_id).unwrap_or_default();

        let mut base_groups = group_by_key(&base_events);

        // Added + matched pairs (matched keys leave `base_groups` empty).
        for (event_key, comp_list) in group_by_key(&comp_events) {
            let Some(base_list) = base_groups.remove(&event_key) else {
                for event in comp_list {
                    diff.events_added.push(UnmatchedEvent {
                        side: Side::B,
                        trace_id: trace_id.clone(),
                        span_id: event.span_id.clone(),
                        operation: event.operation.clone(),
                        offset_ms: event.offset_ms,
                    });
                }
                continue;
            };
            for (base_event, comp_event) in base_list.iter().zip(comp_list.iter()) {
                if let (Some(base_ms), Some(comp_ms)) =
                    (base_event.duration_ms, comp_event.duration_ms)
                {
                    let percent = if base_ms == 0 {
                        if comp_ms == 0 { 0.0 } else { f64::INFINITY }
                    } else {
                        ((comp_ms as f64 - base_ms as f64) / base_ms as f64) * 100.0
                    };
                    if percent.abs() > DURATION_DELTA_THRESHOLD_PERCENT {
                        diff.duration_deltas.push(DurationDelta {
                            trace_id: trace_id.clone(),
                            span_id: event_key.0.clone(),
                            operation: event_key.1.clone(),
                            baseline_ms: base_ms,
                            compared_ms: comp_ms,
                            percent,
                        });
                    }
                }
                if base_event.status != comp_event.status {
                    diff.status_changes.push(StatusChange {
                        trace_id: trace_id.clone(),
                        span_id: event_key.0.clone(),
                        operation: event_key.1.clone(),
                        baseline: base_event.status,
                        compared: comp_event.status,
                    });
                }
            }
        }

        // Whatever remains on the baseline side was removed.
        for base_list in base_groups.into_values() {
            for event in base_list {
                diff.events_removed.push(UnmatchedEvent {
                    side: Side::A,
                    trace_id: trace_id.clone(),
                    span_id: event.span_id.clone(),
                    operation: event.operation.clone(),
                    offset_ms: event.offset_ms,
                });
            }
        }

        // Finish-status mismatch summary for this trace.
        let base_finish = finish_status(&base_events);
        let comp_finish = finish_status(&comp_events);
        if !base_events.is_empty() && !comp_events.is_empty() && base_finish != comp_finish {
            diff.finish_mismatches.push(FinishMismatch {
                trace_id,
                baseline_finish: base_finish,
                compared_finish: comp_finish,
            });
        }
    }

    diff
}

fn push_row(out: &mut String, marker: &str, event: &UnmatchedEvent) {
    out.push_str(&format!(
        "{marker} [{}] trace={} span={} op={} @{}ms\n",
        event.side.label(),
        event.trace_id,
        event.span_id,
        event.operation,
        event.offset_ms
    ));
}

/// Renders the diff as an aligned plain-text table (human output).
pub fn format_diff_table(diff: &RecordingDiff) -> String {
    if diff.is_empty() {
        return "recordings are identical\n".to_string();
    }
    let mut out = String::new();
    out.push_str(&format!("traces compared: {}\n", diff.traces_compared));

    if !diff.events_added.is_empty() || !diff.events_removed.is_empty() {
        out.push_str("\n== structural changes ==\n");
        for event in &diff.events_added {
            push_row(&mut out, "+", event);
        }
        for event in &diff.events_removed {
            push_row(&mut out, "-", event);
        }
    }

    if !diff.duration_deltas.is_empty() {
        out.push_str("\n== duration deltas (>20%) ==\n");
        out.push_str(&format!(
            "{:<12} {:>10} {:>10} {:>9}\n",
            "span", "baseline", "compared", "delta"
        ));
        for delta in &diff.duration_deltas {
            out.push_str(&format!(
                "{:<12} {:>7} ms {:>7} ms {:>+8.1}%\n",
                clip(&delta.span_id, 12),
                delta.baseline_ms,
                delta.compared_ms,
                delta.percent
            ));
        }
    }

    if !diff.status_changes.is_empty() {
        out.push_str("\n== status changes ==\n");
        for change in &diff.status_changes {
            out.push_str(&format!(
                "~ trace={} span={} op={}: {:?} -> {:?}\n",
                change.trace_id, change.span_id, change.operation, change.baseline, change.compared
            ));
        }
    }

    if !diff.finish_mismatches.is_empty() {
        out.push_str("\n== finish-status mismatches ==\n");
        for mismatch in &diff.finish_mismatches {
            out.push_str(&format!(
                "! trace {}: baseline finishes {:?}, compared finishes {:?}\n",
                mismatch.trace_id, mismatch.baseline_finish, mismatch.compared_finish
            ));
        }
    }

    out.push_str(&format!(
        "\ntotal differences: {}\n",
        diff.difference_count()
    ));
    out
}

/// First up-to-`max` characters of `text`, safe on UTF-8 boundaries.
fn clip(text: &str, max: usize) -> String {
    text.chars().take(max).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ai_observability::EventKind;
    use std::collections::BTreeMap;

    /// Builds an event with sensible defaults for diff fixtures.
    fn event(
        trace_id: &str,
        span_id: &str,
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
            parent_span_id: None,
            kind: EventKind::ModelCall,
            operation: operation.to_string(),
            status,
            duration_ms,
            metadata: BTreeMap::new(),
        }
    }

    /// Two diffable runs of the same trace `tX`:
    /// - span `keep` matched (duration 100 -> 150 ms = +50%, status flip),
    /// - span `extra` removed in run B,
    /// - span `fresh` added in run B.
    fn run_a() -> Vec<ExecutionEvent> {
        vec![
            event("tX", "keep", "call", EventStatus::Succeeded, 0, Some(100)),
            event("tX", "extra", "side", EventStatus::Succeeded, 5, Some(10)),
        ]
    }

    fn run_b() -> Vec<ExecutionEvent> {
        vec![
            event("tX", "fresh", "new-op", EventStatus::Succeeded, 0, Some(7)),
            event("tX", "keep", "call", EventStatus::Failed, 4, Some(150)),
        ]
    }

    #[test]
    fn identical_recordings_produce_empty_diff() {
        let events = run_a();
        let diff = diff_recordings(&events, &events);
        assert!(diff.is_empty());
        assert_eq!(diff.difference_count(), 0);
        assert_eq!(diff.traces_compared, 1);
    }

    #[test]
    fn added_and_removed_events_are_reported_per_key() {
        let diff = diff_recordings(&run_a(), &run_b());
        assert_eq!(diff.events_removed.len(), 1);
        assert_eq!(diff.events_removed[0].span_id, "extra");
        assert_eq!(diff.events_removed[0].side, Side::A);
        assert_eq!(diff.events_added.len(), 1);
        assert_eq!(diff.events_added[0].span_id, "fresh");
        assert_eq!(diff.events_added[0].operation, "new-op");
        assert_eq!(diff.events_added[0].side, Side::B);
    }

    #[test]
    fn duration_delta_above_threshold_is_flagged() {
        let diff = diff_recordings(&run_a(), &run_b());
        assert_eq!(diff.duration_deltas.len(), 1, "{diff:?}");
        let delta = &diff.duration_deltas[0];
        assert_eq!(delta.span_id, "keep");
        assert_eq!((delta.baseline_ms, delta.compared_ms), (100, 150));
        assert!((delta.percent - 50.0).abs() < f64::EPSILON);
    }

    #[test]
    fn small_duration_change_stays_below_threshold() {
        let baseline = vec![event("t", "s", "op", EventStatus::Succeeded, 0, Some(100))];
        // +15% stays under the >20% threshold.
        let compared = vec![event("t", "s", "op", EventStatus::Succeeded, 0, Some(115))];
        let diff = diff_recordings(&baseline, &compared);
        assert!(diff.duration_deltas.is_empty(), "{diff:?}");
        // +25% crosses it.
        let compared = vec![event("t", "s", "op", EventStatus::Succeeded, 0, Some(125))];
        let diff = diff_recordings(&baseline, &compared);
        assert_eq!(diff.duration_deltas.len(), 1);
    }

    #[test]
    fn zero_baseline_with_nonzero_compared_flags_infinite_delta() {
        let baseline = vec![event("t", "s", "op", EventStatus::Succeeded, 0, Some(0))];
        let compared = vec![event("t", "s", "op", EventStatus::Succeeded, 0, Some(9))];
        let diff = diff_recordings(&baseline, &compared);
        assert_eq!(diff.duration_deltas.len(), 1);
        assert!(diff.duration_deltas[0].percent.is_infinite());
    }

    #[test]
    fn status_changes_and_finish_mismatch_are_summarized() {
        let diff = diff_recordings(&run_a(), &run_b());
        assert_eq!(diff.status_changes.len(), 1);
        let change = &diff.status_changes[0];
        assert_eq!(change.baseline, EventStatus::Succeeded);
        assert_eq!(change.compared, EventStatus::Failed);

        assert_eq!(diff.finish_mismatches.len(), 1);
        assert_eq!(
            diff.finish_mismatches[0].baseline_finish,
            EventStatus::Succeeded
        );
        assert_eq!(
            diff.finish_mismatches[0].compared_finish,
            EventStatus::Failed
        );
    }

    #[test]
    fn multi_trace_recordings_compare_independently() {
        let mut baseline = run_a();
        baseline.push(event("tY", "r", "root", EventStatus::Succeeded, 0, Some(3)));
        let compared = run_b();
        let diff = diff_recordings(&baseline, &compared);
        assert_eq!(diff.traces_compared, 2);
        // tY vanished entirely on the compared side -> all its events removed.
        assert!(
            diff.events_removed
                .iter()
                .any(|e| e.trace_id == "tY" && e.span_id == "r")
        );
        // The only finish mismatch is tX's status flip; tY has no compared
        // side and therefore contributes none.
        assert_eq!(diff.finish_mismatches.len(), 1);
    }

    #[test]
    fn duplicate_keys_pair_up_positionally() {
        let mk = |offset| event("t", "s", "op", EventStatus::Started, offset, None);
        let baseline = vec![mk(0), mk(1)];
        let compared = vec![mk(2), mk(3)];
        let diff = diff_recordings(&baseline, &compared);
        assert!(diff.is_empty(), "{diff:?}");
    }

    #[test]
    fn human_table_lists_all_sections() {
        let diff = diff_recordings(&run_a(), &run_b());
        let table = format_diff_table(&diff);
        assert!(table.contains("structural changes"), "{table}");
        assert!(table.contains("+ [only-in-b]"), "{table}");
        assert!(table.contains("- [only-in-a]"), "{table}");
        assert!(table.contains("duration deltas (>20%)"), "{table}");
        assert!(table.contains("status changes"), "{table}");
        assert!(table.contains("finish-status mismatches"), "{table}");
        assert!(table.contains("total differences: 5"), "{table}");
    }

    #[test]
    fn human_table_reports_identical_recordings() {
        let table = format_diff_table(&diff_recordings(&run_a(), &run_a()));
        assert_eq!(table, "recordings are identical\n");
    }

    #[test]
    fn finish_status_any_failure_means_failed() {
        let ok = vec![event("t", "s", "op", EventStatus::Succeeded, 0, None)];
        assert_eq!(finish_status(&ok), EventStatus::Succeeded);
        let bad = vec![
            event("t", "s", "op", EventStatus::Failed, 0, None),
            event("t", "s2", "op", EventStatus::Succeeded, 1, None),
        ];
        assert_eq!(finish_status(&bad), EventStatus::Failed);
    }

    #[test]
    fn diff_serializes_to_json() {
        let diff = diff_recordings(&run_a(), &run_b());
        let json = serde_json::to_string(&diff).expect("serializable");
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["traces_compared"], 1);
        assert_eq!(parsed["events_added"][0]["side"], "only-in-b");
        assert_eq!(parsed["status_changes"][0]["baseline"], "succeeded");
        assert_eq!(parsed["finish_mismatches"][0]["compared_finish"], "failed");
    }

    #[test]
    fn renamed_operation_counts_as_remove_plus_add() {
        let baseline = vec![event("t", "s", "old", EventStatus::Succeeded, 0, Some(5))];
        let compared = vec![event("t", "s", "new", EventStatus::Succeeded, 0, Some(5))];
        let diff = diff_recordings(&baseline, &compared);
        assert_eq!(diff.events_removed.len(), 1);
        assert_eq!(diff.events_added.len(), 1);
        assert!(diff.status_changes.is_empty());
    }
}
