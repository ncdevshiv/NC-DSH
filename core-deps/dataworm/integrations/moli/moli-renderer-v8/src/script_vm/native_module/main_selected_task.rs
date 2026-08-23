//! Body-only execution for selected main native-module tasks.
//!
//! A main-Document runtime ticket may advance a dynamic-import graph or consume
//! one module-map/modulepreload owner event. A selected Networking graph
//! terminal can run the same owner-action fanout. This module records their
//! concrete body activity while deliberately leaving the ordinary task-end
//! checkpoint to the Page selected-task dispatcher.

use super::*;

/// Activity produced by one concrete selected native-module body.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MainNativeModuleSelectedTaskBodyActivity {
    StateTransitionOnly,
    PageRealmBodyAttempted,
}

/// Settlement of one concrete native-module task body.
///
/// The error is retained rather than returned immediately because Page code
/// may already have run.  The selected dispatcher must submit the task-end
/// checkpoint before propagating that error to the owner loop.
#[must_use = "selected native-module execution must reach Page task completion"]
#[derive(Debug)]
pub(crate) struct MainNativeModuleSelectedTaskExecution {
    activity: MainNativeModuleSelectedTaskBodyActivity,
    failure: Option<String>,
}

/// Whether a stable main-runtime ticket consumed concrete owner work.
#[must_use = "selected native-module application must be reconciled by PageVm"]
#[derive(Debug)]
pub(crate) enum MainNativeModuleSelectedTaskApplication {
    ReservationSpent,
    Applied(MainNativeModuleSelectedTaskExecution),
}

/// Body-only result of applying one already-authorized main dynamic-import
/// graph terminal and all owner actions it synchronously fans out.
///
/// `owner_actions` remains a typed handoff for runtime module-script failures;
/// `activity` records whether the fanout entered the importing Window realm.
/// Neither fact is scheduler metadata and neither may be stored on a queued
/// Networking task.
#[must_use = "dynamic-import graph body settlement determines resource-task completion"]
pub(crate) struct MainDynamicImportGraphFetchBodySettlement {
    owner_actions: NativeModuleOwnerActions,
    activity: MainNativeModuleSelectedTaskBodyActivity,
}

impl MainDynamicImportGraphFetchBodySettlement {
    pub(crate) fn into_parts(
        self,
    ) -> (
        NativeModuleOwnerActions,
        MainNativeModuleSelectedTaskBodyActivity,
    ) {
        (self.owner_actions, self.activity)
    }
}

impl MainNativeModuleSelectedTaskApplication {
    fn applied(
        activity: MainNativeModuleSelectedTaskBodyActivity,
        result: std::result::Result<(), String>,
    ) -> Self {
        Self::Applied(MainNativeModuleSelectedTaskExecution {
            activity,
            failure: result.err(),
        })
    }
}

impl MainNativeModuleSelectedTaskExecution {
    pub(crate) fn into_parts(self) -> (MainNativeModuleSelectedTaskBodyActivity, Option<String>) {
        (self.activity, self.failure)
    }
}

struct MainNativeModuleSelectedTaskBody {
    activity: MainNativeModuleSelectedTaskBodyActivity,
}

impl Default for MainNativeModuleSelectedTaskBody {
    fn default() -> Self {
        Self {
            activity: MainNativeModuleSelectedTaskBodyActivity::StateTransitionOnly,
        }
    }
}

impl MainNativeModuleSelectedTaskBody {
    const fn activity(&self) -> MainNativeModuleSelectedTaskBodyActivity {
        self.activity
    }
}

impl ScriptVmMainNativeModuleTaskBody for MainNativeModuleSelectedTaskBody {
    fn note_page_realm_body_attempted(&mut self) {
        self.activity = MainNativeModuleSelectedTaskBodyActivity::PageRealmBodyAttempted;
    }

    fn resolve_ready_source_import(
        &mut self,
        vm: &mut ScriptVm,
        request: PendingDynamicModuleImport,
        root_entry: ModuleEntryId,
    ) -> std::result::Result<NativeDynamicModuleSourceImportResolution, ModuleLoadError> {
        vm.resolve_native_dynamic_module_source_import_selected_task_body(request, root_entry)
    }

    fn resolve_completed_evaluation_import(
        &mut self,
        vm: &mut ScriptVm,
        request: PendingDynamicModuleImport,
        target: &DynamicModuleEvaluationTarget,
    ) -> std::result::Result<(), ModuleLoadError> {
        vm.resolve_native_dynamic_module_import_selected_task_body(request, target)
    }

    fn reject_dynamic_import(
        &mut self,
        vm: &mut ScriptVm,
        request: PendingDynamicModuleImport,
        error: &ModuleLoadError,
    ) -> std::result::Result<(), ModuleLoadError> {
        vm.reject_native_dynamic_module_import_with_error_selected_task_body(request, error)
    }
}

impl ScriptVm {
    /// Apply one exact dynamic-import graph terminal without performing the
    /// enclosing Networking task's checkpoint.
    pub(crate) fn apply_current_main_dynamic_import_graph_fetch_completion_selected_task_body(
        &mut self,
        authorization: crate::runtime::AuthorizedCurrentMainDynamicImportGraphFetchCompletion,
    ) -> Result<MainDynamicImportGraphFetchBodySettlement> {
        let completion = authorization.into_completion();
        let target = completion.target();
        assert_eq!(
            self.current_main_dynamic_import_graph_fetch_target(target.load_id()),
            Some(target),
            "authorized main dynamic-import terminal must retain its exact current fetch"
        );
        let mut body = MainNativeModuleSelectedTaskBody::default();
        let owner_actions = self
            .complete_current_main_dynamic_import_graph_fetch_result_with_body(
                target,
                completion.into_result(),
                &mut body,
            )?;
        Ok(MainDynamicImportGraphFetchBodySettlement {
            owner_actions,
            activity: body.activity(),
        })
    }

    /// Install an already-derived owner action batch for a domain handoff
    /// test. This exposes the production state transition only; semantic task
    /// tests must still run the resulting continuations through PageVm's
    /// selected dispatcher.
    #[cfg(test)]
    pub(crate) fn commit_runtime_module_graph_start_actions_for_selected_task_test(
        &mut self,
        actions: NativeModuleOwnerActions,
    ) {
        self.commit_runtime_module_graph_start_actions(actions);
    }

    /// Consume one exact dynamic-module graph ticket without performing its
    /// ordinary task-end checkpoint.
    pub(crate) fn run_next_native_dynamic_module_owner_action_selected_task_body(
        &mut self,
    ) -> MainNativeModuleSelectedTaskApplication {
        let Some(job) = self
            .document_runtime
            .take_next_native_dynamic_module_import()
        else {
            return MainNativeModuleSelectedTaskApplication::ReservationSpent;
        };
        let document_owner = job
            .dynamic_import_request()
            .expect("dynamic module graph job must retain its import request")
            .owner();
        if !self.dynamic_module_import_owner_is_current(document_owner) {
            self.record_runtime_warning(format_args!(
                "dropped stale dynamic import before graph advance: owner={document_owner:?}"
            ));
            return MainNativeModuleSelectedTaskApplication::applied(
                MainNativeModuleSelectedTaskBodyActivity::StateTransitionOnly,
                Ok(()),
            );
        }

        let mut body = MainNativeModuleSelectedTaskBody::default();
        let result = self
            .advance_native_dynamic_module_import_job_with_body(job, &mut body)
            .map_err(|error| error.to_string());
        MainNativeModuleSelectedTaskApplication::applied(body.activity(), result)
    }

    /// Consume one exact native module-map/modulepreload event without
    /// performing its ordinary task-end checkpoint.
    ///
    /// Runtime module failures are transferred back into DynamicScriptOwner;
    /// they are not synchronously dispatched here.  The existing typed runtime
    /// continuation therefore remains the sole terminal executor.
    pub(crate) fn run_next_native_module_owner_event_selected_task_body(
        &mut self,
    ) -> MainNativeModuleSelectedTaskApplication {
        if !self.has_ready_native_module_owner_actions() {
            return MainNativeModuleSelectedTaskApplication::ReservationSpent;
        }
        let Some(event) = self.document_runtime.take_next_native_module_owner_event() else {
            return MainNativeModuleSelectedTaskApplication::ReservationSpent;
        };

        let mut body = MainNativeModuleSelectedTaskBody::default();
        let result = self
            .dispatch_native_module_owner_event_with_body(event, &mut body)
            .map(|(actions, _published_followup)| {
                self.commit_runtime_module_graph_start_actions(actions);
            })
            .map_err(|error| format!("{error:#}"));
        MainNativeModuleSelectedTaskApplication::applied(body.activity(), result)
    }
}
