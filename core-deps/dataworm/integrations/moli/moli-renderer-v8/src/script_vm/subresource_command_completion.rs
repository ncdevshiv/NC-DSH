use anyhow::Result;

use super::{AsyncSubresourceFetchBodyActivity, ScriptVm};
use crate::types::PendingSubresourceContinueEvent;

/// Result of applying one Fetch-interception command body.
///
/// The command body may synchronously settle a Window-owned Fetch/XHR promise
/// or dispatch an event, but it must not decide when the protocol command has
/// ended.  The Page command coordinator consumes this value and performs the
/// checkpoint only when the body actually entered a Window realm. Keeping the
/// output, activity, and any post-checkpoint publication together prevents
/// passive Worker/network-only branches from being mistaken for JS-capable
/// command completions or waking protocol capture too early.
#[must_use = "the enclosing Page command must consume the command execution"]
pub(crate) struct AsyncSubresourceCommandExecution<T> {
    output: T,
    activity: AsyncSubresourceFetchBodyActivity,
    post_checkpoint_event: Option<PendingSubresourceContinueEvent>,
}

impl<T> AsyncSubresourceCommandExecution<T> {
    pub(super) fn without_window_realm(output: T) -> Self {
        Self {
            output,
            activity: AsyncSubresourceFetchBodyActivity::NoWindowRealmEntered,
            post_checkpoint_event: None,
        }
    }

    pub(super) fn after_body(output: T, activity: AsyncSubresourceFetchBodyActivity) -> Self {
        Self {
            output,
            activity,
            post_checkpoint_event: None,
        }
    }

    /// Preserve outputs which the old command boundary published only after
    /// its checkpoint. The event stays inside this single-use execution value,
    /// so a caller cannot wake protocol capture before completing the Window
    /// reactions produced by the command body.
    pub(super) fn with_post_checkpoint_event(
        mut self,
        event: PendingSubresourceContinueEvent,
    ) -> Self {
        self.post_checkpoint_event = Some(event);
        self
    }

    pub(crate) fn into_parts(
        self,
    ) -> (
        T,
        AsyncSubresourceFetchBodyActivity,
        Option<PendingSubresourceContinueEvent>,
    ) {
        (self.output, self.activity, self.post_checkpoint_event)
    }
}

impl ScriptVm {
    /// Submit the command-end checkpoint for a Fetch-interception command that
    /// synchronously entered a Window realm.
    ///
    /// This is a command completion boundary, not an async-resource task body.
    /// Typed Networking terminals use the selected Page-task dispatcher instead.
    pub(crate) fn finish_async_subresource_command_checkpoint(&mut self) -> Result<()> {
        self.perform_owner_lane_task_microtask_checkpoints()
    }

    pub(crate) fn publish_async_subresource_command_event(
        &mut self,
        event: PendingSubresourceContinueEvent,
    ) {
        self._context_host
            .borrow_mut()
            .record_pending_subresource_continue_event(event);
    }

    #[cfg(test)]
    pub(super) fn finish_async_subresource_command_for_test<T>(
        &mut self,
        execution: AsyncSubresourceCommandExecution<T>,
    ) -> Result<T> {
        let (output, activity, post_checkpoint_event) = execution.into_parts();
        if matches!(
            activity,
            AsyncSubresourceFetchBodyActivity::WindowRealmEntered
        ) {
            self.finish_async_subresource_command_checkpoint()?;
        }
        if let Some(event) = post_checkpoint_event {
            self.publish_async_subresource_command_event(event);
        }
        Ok(output)
    }

    #[cfg(test)]
    pub(super) fn finish_async_subresource_body_checkpoint_for_test(
        &mut self,
        activity: AsyncSubresourceFetchBodyActivity,
    ) -> Result<()> {
        if matches!(
            activity,
            AsyncSubresourceFetchBodyActivity::WindowRealmEntered
        ) {
            self.finish_async_subresource_command_checkpoint()?;
        }
        Ok(())
    }
}
