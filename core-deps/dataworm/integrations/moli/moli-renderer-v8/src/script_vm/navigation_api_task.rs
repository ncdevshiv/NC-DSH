use anyhow::{Result, anyhow};

use super::ScriptVm;
use crate::{
    native_bridge::PendingNavigationApiTaskAction,
    page_task_queue::{
        RendererPageNavigationApiTaskId, RendererPageNavigationApiTaskKind,
        RendererPageNavigationApiTaskOwner,
    },
    runtime::AuthorizedCurrentPageNavigationApiTask,
};

/// Proof that one authorized Navigation API task applied its active finish
/// result. Cancellation drains the Host payload before retiring the attempt,
/// so an exact payload paired with an inactive attempt is an invariant breach,
/// not a second successful body outcome.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct NavigationApiTaskBodyApplied(());

impl ScriptVm {
    pub(crate) fn current_pending_navigation_api_task_owner(
        &self,
        task_id: RendererPageNavigationApiTaskId,
        root_document: crate::runtime::RendererDocumentToken,
    ) -> Option<(
        RendererPageNavigationApiTaskOwner,
        RendererPageNavigationApiTaskKind,
    )> {
        let (execution_context, kind) = self
            ._context_host
            .borrow()
            .current_pending_navigation_api_task_owner(task_id)?;
        Some((
            RendererPageNavigationApiTaskOwner::new(root_document, execution_context),
            kind,
        ))
    }

    /// Apply one exact Navigation API task body without ending the selected
    /// Page task.
    ///
    /// The task-end checkpoint, child synchronization, and runtime follow-up
    /// belong to the Page selected-task dispatcher. Navigation's own
    /// microtasks that order `navigatesuccess` and `finished` remain part of
    /// the domain algorithm and run at that final checkpoint.
    pub(crate) fn apply_current_navigation_api_task_body(
        &mut self,
        authorization: AuthorizedCurrentPageNavigationApiTask,
    ) -> Result<NavigationApiTaskBodyApplied> {
        let task = authorization.into_task();
        let owner = task.owner();
        let queued = self
            ._context_host
            .borrow_mut()
            .take_pending_navigation_api_task_for_exact_owner(
                task.task_id(),
                owner.execution_context(),
                task.kind(),
            )
            .ok_or_else(|| anyhow!("authorized Navigation API task lost its exact payload"))?;

        let (bound_owner, bound_dispatch_scope, _realm_token, context) =
            queued.relevant_context.into_parts();
        debug_assert_eq!(queued.execution_context, owner.execution_context());
        debug_assert_eq!(bound_owner, owner.execution_context().owner());
        debug_assert_eq!(
            bound_dispatch_scope,
            owner.execution_context().dispatch_scope()
        );

        let context_ptr: *const v8::Global<v8::Context> = &context;
        self.with_context_scope_by_ptr(context_ptr, move |scope, _host_ptr| {
            let previous_dispatch_scope = bound_dispatch_scope.enter(scope);
            let application = match queued.action {
                PendingNavigationApiTaskAction::FinishResult(result) => {
                    crate::context_bootstrap::apply_pending_navigation_finished_result(
                        scope, result,
                    )
                }
            };
            bound_dispatch_scope.defer_restore(scope, previous_dispatch_scope);
            anyhow::ensure!(
                application
                    == crate::context_bootstrap::NavigationFinishedResultApplication::Applied,
                "authorized Navigation API task retained an inactive navigation attempt"
            );
            Ok(NavigationApiTaskBodyApplied(()))
        })
    }

    pub(crate) fn discard_stale_navigation_api_task(
        &mut self,
        task_id: RendererPageNavigationApiTaskId,
    ) -> bool {
        self._context_host
            .borrow_mut()
            .discard_pending_navigation_api_task(task_id)
    }
}
