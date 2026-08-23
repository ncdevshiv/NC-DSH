//! Compatibility helpers for low-level ScriptVm lifecycle fixtures.
//!
//! These wrappers intentionally skip the production style turn-exit adapter,
//! matching the old direct helper calls. Production lifecycle execution must
//! use the typed body plus PageVm coordinator.

use super::{MainDocumentLifecycleBody, NonScriptPageTaskExecutionOutcome, ScriptVm};
use crate::frame_owner_model::{FrameDocumentTaskOwner, MainDocumentInteractiveLifecycleAction};
use crate::page_task_queue::PageOwnedInternalLoadingTask;

impl ScriptVm {
    pub(crate) fn apply_main_document_interactive_lifecycle_action(
        &mut self,
        action: MainDocumentInteractiveLifecycleAction,
    ) -> Result<(), String> {
        self.execute_main_document_lifecycle_body_inner(MainDocumentLifecycleBody::Interactive(
            action,
        ))
        .into_legacy_post_parse_outcome()
        .map(|_| ())
    }

    pub(crate) fn dispatch_main_document_domcontentloaded_lifecycle(
        &mut self,
        owner: FrameDocumentTaskOwner,
    ) {
        let execution = self.execute_main_document_lifecycle_body_inner(
            MainDocumentLifecycleBody::DomContentLoaded { owner },
        );
        if let Err(failure) = execution.into_completion() {
            panic!(
                "DOMContentLoaded fixture body unexpectedly failed: {}",
                failure.into_message()
            );
        }
    }

    pub(crate) fn dispatch_main_document_window_load_lifecycle(
        &mut self,
        owner: FrameDocumentTaskOwner,
    ) -> Result<Option<PageOwnedInternalLoadingTask>, String> {
        let execution = self.execute_main_document_lifecycle_body_inner(
            MainDocumentLifecycleBody::WindowLoad { owner },
        );
        match execution.into_legacy_post_parse_outcome()? {
            NonScriptPageTaskExecutionOutcome::None => Ok(None),
            NonScriptPageTaskExecutionOutcome::ScheduleInternalLoading { task, ready_at }
                if ready_at <= std::time::Instant::now() =>
            {
                Ok(Some(task))
            }
            NonScriptPageTaskExecutionOutcome::ScheduleInternalLoading { task, ready_at } => {
                Err(format!(
                    "standalone lifecycle fixture cannot schedule a {}ms delayed internal-loading task for {:?}",
                    ready_at
                        .saturating_duration_since(std::time::Instant::now())
                        .as_millis(),
                    task.document_owner()
                ))
            }
        }
    }
}
