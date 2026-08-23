//! Thin crossterm/ratatui plumbing for the trace TUI.
//!
//! This module is deliberately dumb: it maps crossterm events onto the
//! pure [`Key`] enum, drives the event loop, and draws the state from
//! [`super::model::App`]. All logic lives in the view-model; only this
//! file touches a real terminal. Raw-terminal enter/restore sits behind
//! [`TerminalGuard`] so tests (and headless callers) can substitute a
//! no-op implementation and never open a TTY.

use std::io;

use ai_errors::{AiError, InternalError};
use crossterm::event::{Event as CtEvent, KeyCode, KeyEventKind, KeyModifiers, poll, read};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::{Frame, Terminal};

use super::model::{self, App, Key, Screen, UiEvent};

/// Owns the raw-terminal lifecycle so the real backend can be swapped for
/// a no-op in tests or embedded use.
pub trait TerminalGuard {
    fn enter(&mut self) -> io::Result<()>;
    fn restore(&mut self) -> io::Result<()>;
}

/// Real terminal: raw mode + alternate screen on enter, restored on drop.
pub struct CrosstermGuard;

impl TerminalGuard for CrosstermGuard {
    fn enter(&mut self) -> io::Result<()> {
        enable_raw_mode()?;
        execute!(io::stdout(), EnterAlternateScreen)
    }

    fn restore(&mut self) -> io::Result<()> {
        execute!(io::stdout(), LeaveAlternateScreen)?;
        disable_raw_mode()
    }
}

/// No-op guard for headless use (tests never touch a real terminal).
pub struct HeadlessGuard;

impl TerminalGuard for HeadlessGuard {
    fn enter(&mut self) -> io::Result<()> {
        Ok(())
    }

    fn restore(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn tui_error(message: impl Into<String>) -> AiError {
    AiError::Internal(InternalError::new(message.into()))
}

/// Runs the interactive loop against the real terminal.
pub fn run(app: App) -> Result<(), AiError> {
    let mut guard = CrosstermGuard;
    guard
        .enter()
        .map_err(|e| tui_error(format!("cannot enter TUI: {e}")))?;
    let result = drive_loop(app);
    // The terminal MUST be restored even when the loop errored out.
    if let Err(e) = guard.restore() {
        eprintln!("warning: failed to restore terminal: {e}");
    }
    result
}

fn drive_loop(mut app: App) -> Result<(), AiError> {
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))
        .map_err(|e| tui_error(format!("cannot init TUI backend: {e}")))?;
    loop {
        terminal
            .draw(|frame| draw(frame, &app))
            .map_err(|e| tui_error(format!("TUI draw failed: {e}")))?;

        // Poll keeps shutdown latency low; Windows delivers Release key
        // events too, so only Press kinds map to model keys.
        let has_event = poll(std::time::Duration::from_millis(200))
            .map_err(|e| tui_error(format!("TUI poll failed: {e}")))?;
        if !has_event {
            continue;
        }
        match read().map_err(|e| tui_error(format!("TUI read failed: {e}")))? {
            CtEvent::Key(key) if key.kind == KeyEventKind::Press => {
                app.handle_key(map_key(&key));
            }
            _ => {}
        }
        if app.should_quit() {
            return Ok(());
        }
    }
}

/// Maps a crossterm key onto the backend-agnostic [`Key`].
fn map_key(key: &crossterm::event::KeyEvent) -> Key {
    match key.code {
        KeyCode::Up => Key::Up,
        KeyCode::Down => Key::Down,
        KeyCode::Left => Key::Left,
        KeyCode::Right => Key::Right,
        KeyCode::PageUp => Key::PageUp,
        KeyCode::PageDown => Key::PageDown,
        KeyCode::Enter => Key::Enter,
        KeyCode::Esc => Key::Esc,
        KeyCode::Backspace => Key::Backspace,
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => Key::Char('q'),
        KeyCode::Char(c) => Key::Char(c),
        _ => Key::Esc,
    }
}

const STATUS_COLORS: [(&str, Color); 5] = [
    ("Started", Color::Yellow),
    ("Succeeded", Color::Green),
    ("Failed", Color::Red),
    ("Retrying", Color::LightMagenta),
    ("Cancelled", Color::DarkGray),
];

fn status_style(status: &str) -> Style {
    let color = STATUS_COLORS
        .iter()
        .find(|(name, _)| status.starts_with(name))
        .map_or(Color::Reset, |&(_, color)| color);
    Style::default().fg(color)
}

fn block(title: &str) -> Block<'_> {
    Block::default().borders(Borders::ALL).title(Span::styled(
        title.to_string(),
        Style::default().add_modifier(Modifier::BOLD),
    ))
}

fn filter_line(app: &App) -> Line<'static> {
    let hint = if app.is_editing_filter() {
        "filter: ".to_string() + app.filter() + "_  (Enter to apply)"
    } else if app.filter().is_empty() {
        "no filter — '/' to filter by kind/status/op".to_string()
    } else {
        "filter: ".to_string() + app.filter() + "  ('/' to edit)"
    };
    Line::from(Span::raw(hint))
}

/// Draws one frame entirely from the view-model state.
fn draw(frame: &mut Frame, app: &App) {
    match app.screen() {
        Screen::TraceList => draw_trace_list(frame, app),
        Screen::Timeline => draw_timeline(frame, app),
    }
}

fn draw_trace_list(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let layout = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(1),
    ])
    .split(area);

    let traces = app.visible_traces();
    let header = Line::from(Span::styled(
        format!(
            " {} traces  (↑/↓ select, Enter timeline, / filter, q quit)",
            traces.len()
        ),
        Style::default().add_modifier(Modifier::BOLD),
    ));
    frame.render_widget(Paragraph::new(header), layout[0]);

    let top = model::scroll_offset(app.trace_cursor(), traces.len(), layout[1].height as usize);
    let lines: Vec<Line> = traces
        .iter()
        .skip(top)
        .map(|trace| model::format_list_row(trace))
        .map(Line::from)
        .collect();
    frame.render_widget(Paragraph::new(lines).block(block("traces")), layout[1]);
    frame.render_widget(Paragraph::new(filter_line(app)), layout[2]);
}

fn draw_timeline(frame: &mut Frame, app: &App) {
    let trace = app.selected_trace();
    let title = match trace {
        Some(trace) => format!("trace {}", trace.trace_id),
        None => "trace -".to_string(),
    };

    let rows = if app.detail_open() && app.selected_event().is_some() {
        Constraint::Percentage(55)
    } else {
        Constraint::Percentage(100)
    };
    let layout =
        Layout::vertical([Constraint::Length(1), rows, Constraint::Length(1)]).split(frame.area());

    let header = Line::from(Span::styled(
        " ↑/↓ select   PgUp/PgDn jump ±10   Enter detail   ← back   q quit".to_string(),
        Style::default().add_modifier(Modifier::BOLD),
    ));
    frame.render_widget(Paragraph::new(header), layout[0]);

    let events = app.visible_events();
    let viewport_height = layout[1].height.saturating_sub(2) as usize; // borders
    let top = model::scroll_offset(app.event_cursor(), events.len(), viewport_height);
    let lines: Vec<Line> = events
        .iter()
        .enumerate()
        .skip(top)
        .map(|(index, event)| timeline_line(index == app.event_cursor(), event))
        .collect();
    frame.render_widget(Paragraph::new(lines).block(block(&title)), layout[1]);

    if app.detail_open() {
        draw_detail(frame, app, layout[1]);
    }
    frame.render_widget(Paragraph::new(filter_line(app)), layout[2]);
}

fn timeline_line(selected: bool, event: &UiEvent) -> Line<'static> {
    let cursor = if selected { ">" } else { " " };
    let mut spans = Vec::with_capacity(2);
    spans.push(Span::styled(
        format!("{cursor} "),
        Style::default().add_modifier(Modifier::BOLD),
    ));
    // Row tinted by its lifecycle status; the selection adds bold.
    let mut style = status_style(&event.status);
    if selected {
        style = style.add_modifier(Modifier::BOLD);
    }
    spans.push(Span::styled(model::format_timeline_row(event), style));
    Line::from(spans)
}

fn draw_detail(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    // Overlay-style pane: re-render the lower right quadrant of the list.
    let detail_area =
        Layout::vertical([Constraint::Percentage(45), Constraint::Min(1)]).split(area)[1];
    let Some(event) = app.selected_event() else {
        return;
    };
    let mut lines: Vec<Line> = vec![Line::from(Span::styled(
        "event detail",
        Style::default().add_modifier(Modifier::BOLD),
    ))];
    for (name, value) in model::format_detail_fields(event) {
        lines.push(Line::from(vec![
            Span::styled(format!("  {name:<16}"), Style::default().fg(Color::Cyan)),
            Span::raw(value),
        ]));
    }
    if !event.metadata_json.is_empty() && event.metadata_json != "{}" {
        lines.push(Line::from(Span::styled(
            "  metadata:",
            Style::default().fg(Color::Cyan),
        )));
        for row in event.metadata_json.lines() {
            lines.push(Line::from(Span::raw(format!("    {row}"))));
        }
    }
    frame.render_widget(
        Paragraph::new(lines).block(block("detail (Esc to close)")),
        detail_area,
    );
}
