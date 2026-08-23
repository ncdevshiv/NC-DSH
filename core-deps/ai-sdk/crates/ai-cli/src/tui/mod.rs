//! Interactive time-travel TUI for trace recordings (`ai trace --tui`).
//!
//! Architecture: [`model`] holds the entire view-model — screens,
//! selection state machine, filtering, formatting — as pure functions over
//! a backend-agnostic [`model::Key`] enum, fully unit tested. [`render`]
//! contains only the thin crossterm/ratatui plumbing: raw-terminal
//! lifecycle behind the [`render::TerminalGuard`] abstraction (tests and
//! headless callers substitute [`render::HeadlessGuard`]), key mapping,
//! and drawing.

pub mod model;
pub mod render;

use ai_devtools::{Inspector, TraceView};
use ai_errors::AiError;

use model::{App, TraceData, UiEvent};

/// Builds the TUI view-model from loaded traces, redacting free-text
/// fields through the inspector's redactor (same rules as reports).
pub fn build_app(inspector: &Inspector, traces: &[TraceView]) -> Result<App, AiError> {
    let mut data = Vec::with_capacity(traces.len());
    for trace in traces {
        let events: Vec<UiEvent> = trace
            .events
            .iter()
            .map(|event| UiEvent::redacted_from(event, &|text: &str| inspector.redact(text)))
            .collect();
        data.push(TraceData::new(trace.trace_id.clone(), events));
    }
    Ok(App::new(data))
}

/// Runs the interactive TUI until the user quits.
pub fn run(app: App) -> Result<(), AiError> {
    render::run(app)
}
