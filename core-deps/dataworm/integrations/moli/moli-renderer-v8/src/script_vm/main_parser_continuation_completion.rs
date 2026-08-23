//! Main-parser continuation task-end primitives.
//!
//! The parser continuation and an exact DOMContentLoaded successor are two
//! distinct HTML tasks even when Chromium-compatible execution consumes the
//! successor without reopening ordinary scheduler arbitration. This module
//! finishes only the parser task: it checkpoints, optionally reconciles child
//! records created by page code, publishes typed follow-up work, and returns.
//! It never dispatches DCL or executes another Page task.

use anyhow::Result;

use super::ScriptVm;
use crate::style_engine::StyleInvalidationTurnExitBoundary;

enum MainParserContinuationCheckpointKind {
    CheckpointOnly,
    Callback,
}

impl ScriptVm {
    pub(crate) fn finish_main_parser_continuation_checkpoint_only(&mut self) -> Result<()> {
        self.finish_main_parser_continuation_task_end(
            MainParserContinuationCheckpointKind::CheckpointOnly,
        )
    }

    pub(crate) fn finish_main_parser_continuation_callback_checkpoint(&mut self) -> Result<()> {
        self.finish_main_parser_continuation_task_end(
            MainParserContinuationCheckpointKind::Callback,
        )
    }

    fn finish_main_parser_continuation_task_end(
        &mut self,
        kind: MainParserContinuationCheckpointKind,
    ) -> Result<()> {
        // Parser-terminal reactions may publish ordinary Page work. Keep it
        // resident but non-runnable until this bounded completion has returned;
        // the exact DCL handoff is coordinated outside this scope.
        self.document_runtime
            .deferred_page_tasks_mut()
            .enter_scope();

        let checkpoint = self.perform_owner_lane_task_microtask_checkpoints();
        if checkpoint.is_ok() && matches!(kind, MainParserContinuationCheckpointKind::Callback) {
            self.sync_child_browsing_context_records();
        }
        let completion = self.finish_runtime_turn_with_style_drain(
            StyleInvalidationTurnExitBoundary::SelectedPageTask,
            checkpoint,
        );

        self.document_runtime.deferred_page_tasks_mut().exit_scope();
        self.drain_deferred_page_tasks_best_effort();
        completion
    }
}
