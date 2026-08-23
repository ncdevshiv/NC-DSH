//! ScriptVm task-end primitives for generic main-Document post-parse work.
//!
//! PageVm decides which execution fact owes a checkpoint. This module only
//! performs that already-proven boundary. Callback tasks additionally
//! synchronize child records and release deferred Page work after the
//! checkpoint, matching the old callback ordering without synchronously
//! executing a newly-published runtime continuation.

use anyhow::Result;

use super::ScriptVm;
use crate::{
    page_task_queue::MainDocumentPostParseTaskEnd, style_engine::StyleInvalidationTurnExitBoundary,
};

impl ScriptVm {
    pub(crate) fn finish_main_document_post_parse_task_end(
        &mut self,
        task_end: MainDocumentPostParseTaskEnd,
    ) -> Result<()> {
        match task_end {
            MainDocumentPostParseTaskEnd::NoCompletion => Ok(()),
            MainDocumentPostParseTaskEnd::CheckpointOnly => {
                let result = self.perform_owner_lane_task_microtask_checkpoints();
                self.finish_runtime_turn_with_style_drain(
                    StyleInvalidationTurnExitBoundary::NonScriptPageTask,
                    result,
                )
            }
            MainDocumentPostParseTaskEnd::CallbackCheckpoint => {
                let result = self.perform_owner_lane_task_microtask_checkpoints();
                if result.is_ok() {
                    self.sync_child_browsing_context_records();
                }
                // The old event helpers drained this residence after their
                // internal checkpoint even when best-effort dispatch failed.
                // Keep that ordering while moving checkpoint authority here.
                self.drain_deferred_page_tasks_best_effort();
                self.finish_runtime_turn_with_style_drain(
                    StyleInvalidationTurnExitBoundary::NonScriptPageTask,
                    result,
                )
            }
        }
    }
}
