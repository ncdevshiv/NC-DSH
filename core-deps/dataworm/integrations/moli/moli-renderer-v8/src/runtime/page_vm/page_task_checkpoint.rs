//! Task-end checkpoint submission for selected Page tasks.
//!
//! A task body may enter V8, but it must not decide when the surrounding
//! scheduler task ends. Each P5 migration removes the helper-local checkpoint
//! from one complete task family and lets the unique selected-task dispatcher
//! call this component. There is deliberately no family enum, `Legacy` mode,
//! or `AlreadyCheckpointed` flag: migrated families share one task-end
//! contract, while unmigrated families continue to own their existing behavior
//! until their whole execution path can move without creating two checkpoint
//! authorities.
//!
//! This component finishes tasks whose completion ends immediately after the
//! checkpoint. Callback tasks with child-record and runtime-script work after
//! the checkpoint use `page_callback_task_completion` instead; folding those
//! steps into this primitive would silently reorder their established turn.

use anyhow::Result;

use crate::style_engine::StyleInvalidationTurnExitBoundary;

use super::PageVm;

impl PageVm {
    /// Finish one selected Page task after its body has returned.
    ///
    /// The underlying `ScriptVm` primitive preserves the established ordering:
    ///
    /// 1. drain the isolate microtask queue;
    /// 2. report pending Promise rejections;
    /// 3. run the moved checkpoint-end batch;
    /// 4. reconcile owner transitions and turn-exit style invalidations.
    ///
    /// Choose this boundary when the selected task owns an ordinary task-end
    /// checkpoint but its body did not produce callback-specific child-frame
    /// or runtime-script follow-up. This function deliberately does not call
    /// `sync_child_browsing_context_records()` or `finish_host_task_turn()`;
    /// callers that dispatched a callback whose reactions can produce those
    /// consequences must use `finish_selected_page_callback_task()` instead.
    ///
    /// Document-replacement reconciliation remains outside this function at
    /// the common owner boundary, so replacement caused by a microtask is
    /// observed before the Page residence is restored.
    pub(super) fn finish_selected_page_task_checkpoint(&mut self) -> Result<()> {
        self.vm_mut().finish_selected_page_task_checkpoint(
            StyleInvalidationTurnExitBoundary::SelectedPageTask,
        )?;
        self.absorb_parser_no_execution_runs();
        Ok(())
    }
}
