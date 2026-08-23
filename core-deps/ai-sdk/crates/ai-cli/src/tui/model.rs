//! Pure view-model for the time-travel trace TUI (`ai trace --tui`).
//!
//! Everything interactive lives here as testable state transitions over a
//! [`Key`] input enum — no crossterm/ratatui types appear in this module,
//! so the selection state machine, filtering, and all formatting are unit
//! tested headlessly. The thin terminal plumbing lives in
//! [`super::render`].

use ai_observability::ExecutionEvent;

/// How far PgUp/PgDn jump the selection.
pub const PAGE_STEP: usize = 10;

/// A terminal key, decoupled from crossterm so tests need no backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Up,
    Down,
    Left,
    Right,
    PageUp,
    PageDown,
    Enter,
    Esc,
    Backspace,
    Char(char),
}

/// Which screen the TUI shows. The event detail is a pane toggled on top
/// of the [`Screen::Timeline`] screen rather than a separate screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    /// All traces grouped by id, with counts and durations.
    TraceList,
    /// One trace's events, chronologically.
    Timeline,
}

/// One event prepared for display (operation and metadata already passed
/// through redaction).
#[derive(Debug, Clone, PartialEq)]
pub struct UiEvent {
    pub offset_ms: u64,
    pub kind: String,
    pub op: String,
    pub status: String,
    pub span_id: String,
    pub parent_span_id: Option<String>,
    pub wall_time: String,
    pub duration_ms: Option<u64>,
    pub metadata_json: String,
}

impl UiEvent {
    /// Maps a recorded event into display form, applying `redact` to every
    /// free-text field (operation and serialized metadata).
    pub fn redacted_from(event: &ExecutionEvent, redact: &dyn Fn(&str) -> String) -> Self {
        let metadata_json = serde_json::to_string_pretty(&event.metadata)
            .map(|json| redact(&json))
            .unwrap_or_else(|_| "{}".to_string());
        Self {
            offset_ms: event.offset_ms,
            kind: format!("{:?}", event.kind),
            op: redact(&event.operation),
            status: format!("{:?}", event.status),
            span_id: event.span_id.clone(),
            parent_span_id: event.parent_span_id.clone(),
            wall_time: event.wall_time.clone(),
            duration_ms: event.duration_ms,
            metadata_json,
        }
    }
}

/// One trace's full event list plus derived summary numbers.
#[derive(Debug, Clone)]
pub struct TraceData {
    pub trace_id: String,
    /// Events sorted by `offset_ms`.
    pub events: Vec<UiEvent>,
    /// Last offset minus first offset within the trace.
    pub duration_ms: u64,
    /// True when any event failed.
    pub failed: bool,
}

impl TraceData {
    pub fn new(trace_id: impl Into<String>, mut events: Vec<UiEvent>) -> Self {
        events.sort_by_key(|e| e.offset_ms);
        let duration_ms = match (events.first(), events.last()) {
            (Some(first), Some(last)) => last.offset_ms.saturating_sub(first.offset_ms),
            _ => 0,
        };
        let failed = events.iter().any(|e| e.status == "Failed");
        Self {
            trace_id: trace_id.into(),
            events,
            duration_ms,
            failed,
        }
    }
}

/// The complete TUI state machine.
#[derive(Debug, Clone)]
pub struct App {
    traces: Vec<TraceData>,
    screen: Screen,
    trace_cursor: usize,
    event_cursor: usize,
    detail_open: bool,
    filter: String,
    editing_filter: bool,
    quit: bool,
}

impl App {
    pub fn new(traces: Vec<TraceData>) -> Self {
        Self {
            traces,
            screen: Screen::TraceList,
            trace_cursor: 0,
            event_cursor: 0,
            detail_open: false,
            filter: String::new(),
            editing_filter: false,
            quit: false,
        }
    }

    pub fn screen(&self) -> Screen {
        self.screen
    }

    pub fn should_quit(&self) -> bool {
        self.quit
    }

    pub fn is_editing_filter(&self) -> bool {
        self.editing_filter
    }

    pub fn filter(&self) -> &str {
        &self.filter
    }

    pub fn detail_open(&self) -> bool {
        self.detail_open
    }

    pub fn trace_cursor(&self) -> usize {
        self.trace_cursor
    }

    pub fn event_cursor(&self) -> usize {
        self.event_cursor
    }

    /// Traces surviving the current filter (id substring or any matching
    /// event), in insertion order.
    pub fn visible_traces(&self) -> Vec<&TraceData> {
        self.traces
            .iter()
            .filter(|trace| self.trace_visible(trace))
            .collect()
    }

    /// Filtered events of the currently selected visible trace.
    pub fn visible_events(&self) -> Vec<&UiEvent> {
        self.selected_trace()
            .map(|trace| {
                trace
                    .events
                    .iter()
                    .filter(|e| self.event_matches(e))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The event under the timeline cursor, if any.
    pub fn selected_event(&self) -> Option<&UiEvent> {
        let events = self.visible_events();
        events
            .get(self.event_cursor.min(events.len().saturating_sub(1)))
            .copied()
    }

    /// The currently selected visible trace, if any.
    pub fn selected_trace(&self) -> Option<&TraceData> {
        let visible = self.visible_traces();
        visible
            .get(self.trace_cursor.min(visible.len().saturating_sub(1)))
            .copied()
    }

    fn trace_visible(&self, trace: &TraceData) -> bool {
        self.filter.is_empty()
            || trace
                .trace_id
                .to_lowercase()
                .contains(&self.filter.to_lowercase())
            || trace.events.iter().any(|e| self.event_matches(e))
    }

    /// Case-insensitive substring match over kind, status, and operation.
    fn event_matches(&self, event: &UiEvent) -> bool {
        if self.filter.is_empty() {
            return true;
        }
        let needle = self.filter.to_lowercase();
        event.kind.to_lowercase().contains(&needle)
            || event.status.to_lowercase().contains(&needle)
            || event.op.to_lowercase().contains(&needle)
    }

    /// Advances the state machine by one key press.
    pub fn handle_key(&mut self, key: Key) {
        if key == Key::Char('q') && !self.editing_filter {
            self.quit = true;
            return;
        }
        if self.editing_filter {
            self.handle_filter_key(key);
            return;
        }
        if key == Key::Char('/') {
            self.editing_filter = true;
            return;
        }
        match self.screen {
            Screen::TraceList => self.handle_list_key(key),
            Screen::Timeline => self.handle_timeline_key(key),
        }
        self.clamp_cursors();
    }

    fn handle_filter_key(&mut self, key: Key) {
        match key {
            Key::Enter | Key::Esc => self.editing_filter = false,
            Key::Backspace => {
                self.filter.pop();
            }
            Key::Char(c) => self.filter.push(c),
            _ => {}
        }
        self.clamp_cursors();
    }

    fn handle_list_key(&mut self, key: Key) {
        let max = self.visible_traces().len().saturating_sub(1);
        match key {
            Key::Up => {
                self.trace_cursor = self.trace_cursor.saturating_sub(1).min(max);
            }
            Key::Down => {
                self.trace_cursor = (self.trace_cursor + 1).min(max);
            }
            Key::PageUp => {
                self.trace_cursor = self.trace_cursor.saturating_sub(PAGE_STEP).min(max);
            }
            Key::PageDown => {
                self.trace_cursor = (self.trace_cursor + PAGE_STEP).min(max);
            }
            Key::Right | Key::Enter if !self.visible_traces().is_empty() => {
                self.screen = Screen::Timeline;
                self.event_cursor = 0;
                self.detail_open = false;
            }
            _ => {}
        }
    }

    fn handle_timeline_key(&mut self, key: Key) {
        let max = self.visible_events().len().saturating_sub(1);
        match key {
            // Per spec, Left always returns to the trace list.
            Key::Left => self.back_to_list(),
            Key::Esc => {
                if self.detail_open {
                    self.detail_open = false;
                } else {
                    self.back_to_list();
                }
            }
            Key::Up => self.step_cursor(max, -1),
            Key::Down => self.step_cursor(max, 1),
            Key::PageUp => self.step_cursor(max, -(PAGE_STEP as isize)),
            Key::PageDown => self.step_cursor(max, PAGE_STEP as isize),
            Key::Right | Key::Enter => self.detail_open = !self.detail_open,
            _ => {}
        }
    }

    fn back_to_list(&mut self) {
        self.screen = Screen::TraceList;
        self.event_cursor = 0;
        self.detail_open = false;
    }

    fn step_cursor(&mut self, max: usize, delta: isize) {
        if max == 0 {
            self.event_cursor = 0;
            return;
        }
        let next = self.event_cursor as isize + delta;
        self.event_cursor = next.clamp(0, max as isize) as usize;
    }

    fn clamp_cursors(&mut self) {
        let trace_len = self.visible_traces().len();
        self.trace_cursor = self.trace_cursor.min(trace_len.saturating_sub(1));
        let event_len = self.visible_events().len();
        self.event_cursor = self.event_cursor.min(event_len.saturating_sub(1));
    }
}

/// Viewport scroll offset keeping `cursor` visible within `height` rows.
pub fn scroll_offset(cursor: usize, len: usize, height: usize) -> usize {
    if height == 0 || len == 0 {
        return 0;
    }
    let cursor = cursor.min(len.saturating_sub(1));
    if cursor < height {
        0
    } else {
        cursor + 1 - height
    }
}

/// Formats a millisecond duration for summaries (`830 ms`, `2.40 s`).
pub fn format_duration(ms: u64) -> String {
    if ms >= 1000 {
        format!("{:.2} s", ms as f64 / 1000.0)
    } else {
        format!("{ms} ms")
    }
}

/// Formats one row of the trace-list screen.
pub fn format_list_row(trace: &TraceData) -> String {
    format!(
        "{:<38} {:>5} events  {:>9}  {}",
        clip(&trace.trace_id, 38),
        trace.events.len(),
        format_duration(trace.duration_ms),
        if trace.failed { "FAILED" } else { "ok" },
    )
}

/// Formats one row of the timeline screen.
pub fn format_timeline_row(event: &UiEvent) -> String {
    format!(
        "{:>8}  {:<14} {:<12} {}",
        format!("{}ms", event.offset_ms),
        clip(&event.kind, 14),
        clip(&event.status, 12),
        event.op,
    )
}

/// Field/value pairs for the detail pane (already ordered for display).
pub fn format_detail_fields(event: &UiEvent) -> Vec<(&'static str, String)> {
    vec![
        ("offset_ms", event.offset_ms.to_string()),
        ("kind", event.kind.clone()),
        ("status", event.status.clone()),
        ("operation", event.op.clone()),
        (
            "duration_ms",
            event
                .duration_ms
                .map(|d| d.to_string())
                .unwrap_or_else(|| "-".to_string()),
        ),
        ("span_id", event.span_id.clone()),
        (
            "parent_span_id",
            event
                .parent_span_id
                .clone()
                .unwrap_or_else(|| "-".to_string()),
        ),
        ("wall_time", event.wall_time.clone()),
    ]
}

fn clip(text: &str, max: usize) -> String {
    text.chars().take(max).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ui_event(offset_ms: u64, kind: &str, op: &str, status: &str) -> UiEvent {
        UiEvent {
            offset_ms,
            kind: kind.to_string(),
            op: op.to_string(),
            status: status.to_string(),
            span_id: format!("span-{offset_ms}"),
            parent_span_id: None,
            wall_time: "2025-01-01T00:00:00Z".to_string(),
            duration_ms: Some(5),
            metadata_json: "{}".to_string(),
        }
    }

    fn sample_app() -> App {
        App::new(vec![
            TraceData::new(
                "trace-alpha",
                vec![
                    ui_event(0, "RequestStarted", "request", "Started"),
                    ui_event(3, "ModelCall", "greet sk-secret123456789", "Succeeded"),
                    ui_event(9, "Completed", "request", "Succeeded"),
                ],
            ),
            TraceData::new(
                "trace-beta",
                vec![
                    ui_event(20, "Metric", "tokens", "Succeeded"),
                    ui_event(25, "ToolCall", "calculator", "Failed"),
                ],
            ),
        ])
    }

    // ---- selection state machine -------------------------------------

    #[test]
    fn starts_on_trace_list_with_first_trace_selected() {
        let app = sample_app();
        assert_eq!(app.screen(), Screen::TraceList);
        assert_eq!(app.visible_traces()[0].trace_id, "trace-alpha");
        assert!(!app.should_quit());
    }

    #[test]
    fn list_navigation_moves_and_clamps() {
        let mut app = sample_app();
        app.handle_key(Key::Down);
        assert_eq!(app.trace_cursor(), 1);
        app.handle_key(Key::Down);
        app.handle_key(Key::Down); // clamped at last trace
        assert_eq!(app.trace_cursor(), 1);
        app.handle_key(Key::Up);
        assert_eq!(app.trace_cursor(), 0);
        app.handle_key(Key::Up); // clamped at first trace
        assert_eq!(app.trace_cursor(), 0);
    }

    #[test]
    fn page_keys_jump_ten_rows() {
        let mut traces = Vec::new();
        for index in 0..25 {
            traces.push(TraceData::new(
                format!("t{index:02}"),
                vec![ui_event(0, "Metric", "m", "Succeeded")],
            ));
        }
        let mut app = App::new(traces);
        app.handle_key(Key::PageDown);
        assert_eq!(app.trace_cursor(), PAGE_STEP);
        app.handle_key(Key::PageDown);
        assert_eq!(app.trace_cursor(), 2 * PAGE_STEP);
        app.handle_key(Key::PageUp);
        assert_eq!(app.trace_cursor(), PAGE_STEP);
        app.handle_key(Key::PageUp);
        app.handle_key(Key::PageUp); // clamps at 0
        assert_eq!(app.trace_cursor(), 0);
    }

    #[test]
    fn enter_opens_timeline_left_returns_to_list() {
        let mut app = sample_app();
        app.handle_key(Key::Enter);
        assert_eq!(app.screen(), Screen::Timeline);
        app.handle_key(Key::Down);
        assert_eq!(app.event_cursor(), 1);
        app.handle_key(Key::Left);
        assert_eq!(app.screen(), Screen::TraceList);
        assert_eq!(app.event_cursor(), 0);
    }

    #[test]
    fn esc_walks_back_through_detail_then_screens() {
        let mut app = sample_app();
        app.handle_key(Key::Enter); // timeline
        app.handle_key(Key::Enter); // open detail
        assert!(app.detail_open());
        app.handle_key(Key::Esc); // closes detail only
        assert!(!app.detail_open());
        assert_eq!(app.screen(), Screen::Timeline);
        app.handle_key(Key::Esc); // back to list
        assert_eq!(app.screen(), Screen::TraceList);
    }

    #[test]
    fn timeline_selection_follows_filtered_events_only() {
        let mut app = sample_app();
        app.set_filter_for_test("toolcall");
        app.handle_key(Key::Enter); // trace-beta matches via its ToolCall event
        assert_eq!(app.screen(), Screen::Timeline);
        assert_eq!(app.visible_events().len(), 1);
        app.handle_key(Key::Down); // single row: cannot move
        assert_eq!(app.event_cursor(), 0);
        assert_eq!(app.selected_event().unwrap().op, "calculator");
    }

    #[test]
    fn q_quits_but_not_while_typing_filter() {
        let mut app = sample_app();
        app.handle_key(Key::Char('/'));
        assert!(app.is_editing_filter());
        app.handle_key(Key::Char('q'));
        assert!(!app.should_quit(), "q while typing belongs in the filter");
        assert_eq!(app.filter(), "q");
        app.handle_key(Key::Esc);
        app.handle_key(Key::Char('q'));
        assert!(app.should_quit());
    }

    #[test]
    fn empty_data_is_safe_to_navigate() {
        let mut app = App::new(vec![]);
        app.handle_key(Key::Down);
        app.handle_key(Key::Enter);
        app.handle_key(Key::Down);
        assert_eq!(app.screen(), Screen::TraceList);
        assert!(app.visible_events().is_empty());
        assert!(app.selected_event().is_none());
    }

    // ---- filtering ----------------------------------------------------

    #[test]
    fn filter_matches_kind_status_and_operation_case_insensitively() {
        let app = sample_app();
        let probe = |needle: &str, op: &str, kind: &str, status: &str| {
            let event = ui_event(0, kind, op, status);
            let filtered = App {
                filter: needle.to_string(),
                ..app.clone()
            };
            filtered.event_matches(&event)
        };
        assert!(probe("model", "x", "ModelCall", "Succeeded"));
        assert!(probe("FAILED", "x", "Metric", "Failed"));
        assert!(probe("CALC", "calculator", "ToolCall", "Succeeded"));
        assert!(!probe("nomatch", "calculator", "ToolCall", "Succeeded"));
    }

    #[test]
    fn filter_narrows_both_screens_and_clamps_cursors() {
        let mut app = sample_app();
        app.handle_key(Key::Down); // select trace-beta
        app.handle_key(Key::Char('/'));
        app.handle_key(Key::Char('a'));
        app.handle_key(Key::Char('l'));
        app.handle_key(Key::Char('p'));
        app.handle_key(Key::Enter); // commit "alp"
        assert_eq!(app.filter(), "alp");
        // Only trace-alpha survives ("alpha" substring); cursor clamps to it.
        assert_eq!(app.visible_traces().len(), 1);
        assert_eq!(app.trace_cursor(), 0);
        assert_eq!(app.visible_traces()[0].trace_id, "trace-alpha");
    }

    #[test]
    fn filter_by_status_substring_selects_failed_trace() {
        let mut app = sample_app();
        app.handle_key(Key::Char('/'));
        for c in "fail".chars() {
            app.handle_key(Key::Char(c));
        }
        app.handle_key(Key::Enter);
        assert_eq!(app.visible_traces().len(), 1);
        assert_eq!(app.visible_traces()[0].trace_id, "trace-beta");
    }

    #[test]
    fn backspace_edits_filter_down_to_empty_match_all() {
        let mut app = sample_app();
        app.handle_key(Key::Char('/'));
        app.handle_key(Key::Char('z'));
        app.handle_key(Key::Char('z'));
        assert!(app.visible_traces().is_empty());
        app.handle_key(Key::Backspace);
        app.handle_key(Key::Backspace);
        app.handle_key(Key::Enter);
        assert_eq!(app.visible_traces().len(), 2, "empty filter shows all");
    }

    // ---- view-model construction & redaction ---------------------------

    #[test]
    fn redacted_from_applies_redaction_to_text_fields() {
        let event = ExecutionEvent {
            wall_time: "2025-06-01T00:00:00Z".into(),
            offset_ms: 42,
            trace_id: "t".into(),
            span_id: "s".into(),
            parent_span_id: Some("root".into()),
            kind: ai_observability::EventKind::ModelCall,
            operation: "key sk-abcdef123456789xyz here".into(),
            status: ai_observability::EventStatus::Succeeded,
            duration_ms: Some(7),
            metadata: std::collections::BTreeMap::from([(
                "authorization".into(),
                serde_json::json!("Bearer abcdefghijklmnop"),
            )]),
        };
        let identity = |text: &str| text.to_string();
        let ui = UiEvent::redacted_from(&event, &identity);
        assert!(
            ui.op.contains("sk-abcdef123456789xyz"),
            "identity keeps text"
        );

        let starred = |text: &str| {
            text.replace("sk-abcdef123456789xyz", "[REDACTED]")
                .replace("abcdefghijklmnop", "[REDACTED]")
        };
        let ui = UiEvent::redacted_from(&event, &starred);
        assert!(!ui.op.contains("sk-abcdef123456789xyz"), "{}", ui.op);
        assert!(ui.op.contains("[REDACTED]"));
        assert!(
            !ui.metadata_json.contains("abcdefghijklmnop"),
            "{}",
            ui.metadata_json
        );
        assert_eq!(ui.offset_ms, 42);
        assert_eq!(ui.kind, "ModelCall");
        assert_eq!(ui.status, "Succeeded");
    }

    #[test]
    fn trace_data_sorts_events_and_derives_summary() {
        let data = TraceData::new(
            "t",
            vec![
                ui_event(50, "Completed", "done", "Succeeded"),
                ui_event(10, "AgentStep", "work", "Failed"),
                ui_event(0, "RequestStarted", "begin", "Started"),
            ],
        );
        let offsets: Vec<u64> = data.events.iter().map(|e| e.offset_ms).collect();
        assert_eq!(offsets, vec![0, 10, 50], "events sorted by offset");
        assert_eq!(data.duration_ms, 50);
        assert!(data.failed, "any Failed marks the trace failed");
    }

    // ---- scrolling & formatting ---------------------------------------

    #[test]
    fn scroll_offset_keeps_cursor_visible() {
        assert_eq!(scroll_offset(0, 100, 10), 0);
        assert_eq!(scroll_offset(9, 100, 10), 0);
        assert_eq!(scroll_offset(10, 100, 10), 1);
        assert_eq!(scroll_offset(99, 100, 10), 90);
        assert_eq!(scroll_offset(500, 3, 10), 0, "clamped to content");
        assert_eq!(scroll_offset(4, 100, 0), 0, "zero viewport is safe");
    }

    #[test]
    fn format_duration_switches_units() {
        assert_eq!(format_duration(830), "830 ms");
        assert_eq!(format_duration(1000), "1.00 s");
        assert_eq!(format_duration(2400), "2.40 s");
    }

    #[test]
    fn format_list_row_shows_count_duration_and_health() {
        let healthy = TraceData::new("abc", vec![ui_event(0, "Metric", "m", "Succeeded")]);
        let failing = TraceData::new("def", vec![ui_event(0, "ToolCall", "t", "Failed")]);
        let row = format_list_row(&healthy);
        assert!(row.contains("abc"), "{row}");
        assert!(row.contains("1 events"), "{row}");
        assert!(row.contains("0 ms"), "{row}");
        assert!(row.contains("ok"), "{row}");
        assert!(format_list_row(&failing).contains("FAILED"));
    }

    #[test]
    fn format_timeline_row_aligns_columns() {
        let row = format_timeline_row(&ui_event(1234, "ModelCall", "gpt call", "Succeeded"));
        assert!(row.contains("1234ms"), "{row}");
        assert!(row.contains("ModelCall"), "{row}");
        assert!(row.contains("Succeeded"), "{row}");
        assert!(row.contains("gpt call"), "{row}");
    }

    #[test]
    fn format_detail_lists_every_field_in_order() {
        let mut event = ui_event(77, "Retry", "backoff", "Retrying");
        event.duration_ms = None;
        event.parent_span_id = None;
        let fields = format_detail_fields(&event);
        let names: Vec<_> = fields.iter().map(|(name, _)| *name).collect();
        assert_eq!(
            names,
            vec![
                "offset_ms",
                "kind",
                "status",
                "operation",
                "duration_ms",
                "span_id",
                "parent_span_id",
                "wall_time"
            ]
        );
        let value = |name: &str| {
            fields
                .iter()
                .find(|(n, _)| *n == name)
                .map(|(_, v)| v.clone())
                .unwrap()
        };
        assert_eq!(value("offset_ms"), "77");
        assert_eq!(value("duration_ms"), "-", "missing duration renders dash");
        assert_eq!(value("parent_span_id"), "-");
        assert_eq!(value("kind"), "Retry");
    }

    impl App {
        /// Test seam: set the filter directly (the TUI edits it key by key).
        fn set_filter_for_test(&mut self, filter: &str) {
            self.filter = filter.to_string();
        }
    }
}
