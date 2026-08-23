//! The planner seam: the contract between the planning side (wave B1:
//! [`clarifier`]/[`expander`]) and the supervision side (wave B2:
//! [`orchestra`]).
//!
//! Wave B1 implements [`Planner`] with model-driven behaviour; wave B2's
//! control loop consumes only this trait, so both waves build in parallel
//! against a fixed seam. Everything here is offline-testable: implementors
//! are expected to back the trait with scripted models in tests.

use ai_errors::AiError;
use async_trait::async_trait;

use crate::mailbox::Question;
use crate::tree::{TaskId, TaskTree};

/// Outcome of the ambiguity gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClarifyVerdict {
    /// `true` when the prompt is executable as stated.
    pub clear: bool,
    /// Why the verdict was reached (short human-readable rationale).
    pub rationale: String,
    /// When not clear: the questions that must be answered before
    /// expansion. Empty when `clear`.
    pub questions: Vec<PendingQuestion>,
}

/// A clarifying question derived from an ambiguous prompt.
///
/// Convertible into a [`Question`] so it can flow through the
/// [`QuestionMailbox`](crate::mailbox::QuestionMailbox).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingQuestion {
    pub text: String,
    pub options: Vec<String>,
}

impl PendingQuestion {
    pub fn into_question(self, id: u64) -> Question {
        Question {
            id,
            text: self.text,
            options: self.options,
        }
    }
}

impl Default for ClarifyVerdict {
    fn default() -> Self {
        Self {
            clear: true,
            rationale: "no assessment performed".into(),
            questions: Vec::new(),
        }
    }
}

/// The planning seam consumed by the orchestrator loop.
///
/// Implementations combine an ambiguity assessment and a decomposition
/// pass. Both methods take the model-facing pieces they need at
/// construction time; the orchestrator never sees provider details.
#[async_trait]
pub trait Planner: Send + Sync {
    /// Assess whether `prompt` is clear enough to expand. Ambiguous
    /// prompts return `clear == false` plus the questions to ask.
    async fn assess(&self, prompt: &str) -> Result<ClarifyVerdict, AiError>;

    /// Expand a *clarified* prompt into new nodes under `parent`
    /// (`None` = roots). Returns the ids of all created leaf nodes —
    /// categories and subcategories may be created as internal nodes,
    /// but only leaves are returned for scheduling.
    async fn expand(
        &self,
        tree: &mut TaskTree,
        parent: Option<TaskId>,
        clarified_prompt: &str,
    ) -> Result<Vec<TaskId>, AiError>;
}
