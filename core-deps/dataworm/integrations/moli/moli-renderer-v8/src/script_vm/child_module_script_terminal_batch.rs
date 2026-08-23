use crate::frame_owner_model::{
    FrameDocumentModuleDependencyTerminalWork, FrameDocumentModuleScriptTerminalFollowup,
    FrameDocumentModuleScriptTerminalHooks, FrameDocumentModuleScriptTerminalOutcome,
    FrameDocumentModuleScriptTerminalRunner, FrameDocumentModuleScriptTerminalWork,
    FrameDocumentParserRootTerminalWork, FrameDocumentTaskOwner, FrameRealmId,
};

use super::ScriptVm;

pub(super) struct ChildModuleScriptTerminalBatchOwner<'vm> {
    vm: &'vm mut ScriptVm,
}

impl<'vm> ChildModuleScriptTerminalBatchOwner<'vm> {
    pub(super) fn new(vm: &'vm mut ScriptVm) -> Self {
        Self { vm }
    }

    pub(super) fn apply_current_terminal_batch_task(
        &mut self,
        authorization: crate::runtime::AuthorizedCurrentChildModuleScriptTerminal,
    ) -> FrameDocumentModuleScriptTerminalOutcome {
        FrameDocumentModuleScriptTerminalRunner::new(ScriptVmChildModuleScriptTerminalHooks {
            vm: self.vm,
        })
        .run_terminal_batch_task(authorization.into_terminal())
    }
}

impl ScriptVm {
    pub(crate) fn current_child_module_script_terminal_owner(
        &self,
        owner: FrameDocumentTaskOwner,
        realm_id: FrameRealmId,
    ) -> Option<FrameDocumentTaskOwner> {
        self._context_host
            .borrow()
            .current_child_module_route_task_owner(owner.document_owner(), realm_id)
    }

    pub(crate) fn current_child_module_terminal_work_is_runnable(
        &self,
        owner: FrameDocumentTaskOwner,
    ) -> bool {
        self._context_host
            .borrow()
            .current_child_module_terminal_work_is_runnable(owner)
    }

    pub(crate) fn apply_current_child_module_script_terminal(
        &mut self,
        authorization: crate::runtime::AuthorizedCurrentChildModuleScriptTerminal,
    ) -> FrameDocumentModuleScriptTerminalOutcome {
        ChildModuleScriptTerminalBatchOwner::new(self)
            .apply_current_terminal_batch_task(authorization)
    }

    /// Apply one production terminal body in low-level ScriptVm semantic
    /// fixtures. This does not submit a Page task-end checkpoint; Page-root
    /// admission, completion, liveness and fairness remain covered by the
    /// production selected dispatcher and owner-scheduler integration tests.
    #[cfg(test)]
    pub(crate) fn run_child_module_script_terminal_body_for_test(&mut self) -> bool {
        let source = self
            ._page_task_residence_for_executor_test
            .as_ref()
            .expect("child module-terminal fixture must retain its production Page source")
            .task_sources();
        let Some(task) = source.take_child_module_script_terminal_for_executor_test(|owner| {
            let current = self.current_child_module_script_terminal_owner(
                owner.document_owner(),
                owner.realm_id(),
            );
            let runnable =
                self.current_child_module_terminal_work_is_runnable(owner.document_owner());
            current != Some(owner.document_owner()) || runnable
        }) else {
            return false;
        };
        let owner = task.owner();
        if self.current_child_module_script_terminal_owner(owner.document_owner(), owner.realm_id())
            == Some(owner.document_owner())
        {
            let _ = self.apply_current_child_module_script_terminal(
                crate::runtime::AuthorizedCurrentChildModuleScriptTerminal::new_for_executor_test(
                    task.into_terminal(),
                ),
            );
        }
        true
    }
}

struct ScriptVmChildModuleScriptTerminalHooks<'vm> {
    vm: &'vm mut ScriptVm,
}

impl FrameDocumentModuleScriptTerminalHooks for ScriptVmChildModuleScriptTerminalHooks<'_> {
    fn handle_parser_root_terminal(
        &mut self,
        work: Box<FrameDocumentParserRootTerminalWork>,
    ) -> FrameDocumentModuleScriptTerminalFollowup {
        let mut owner =
            super::child_module_script_terminal::ChildModuleScriptTerminalOwner::new(self.vm);
        owner.handle_parser_root_terminal_work(*work)
    }

    fn handle_single_module_terminal(
        &mut self,
        work: FrameDocumentModuleScriptTerminalWork,
    ) -> FrameDocumentModuleScriptTerminalFollowup {
        let mut owner =
            super::child_module_script_terminal::ChildModuleScriptTerminalOwner::new(self.vm);
        owner.handle_single_module_script_terminal_work(work)
    }

    fn handle_dependency_terminal(
        &mut self,
        work: Box<FrameDocumentModuleDependencyTerminalWork>,
    ) -> FrameDocumentModuleScriptTerminalFollowup {
        let mut owner =
            super::child_module_script_terminal::ChildModuleScriptTerminalOwner::new(self.vm);
        owner.handle_dependency_terminal_work(*work)
    }
}
