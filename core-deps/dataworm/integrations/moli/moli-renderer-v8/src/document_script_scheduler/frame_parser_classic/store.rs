use std::collections::BTreeMap;

use super::runner::FrameParserClassicScriptRunner;
use crate::{
    document_runtime::DomHandle,
    document_script_scheduler::{
        FrameDocumentClassicScriptSchedulerWork, FrameParserClassicScriptItem,
    },
    frame_owner_model::{
        FrameDocumentClassicScriptBeginExecutionAction, FrameDocumentClassicScriptCompletionAction,
        FrameDocumentClassicScriptScheduling, FrameDocumentClassicScriptSourceLoadClient,
        FrameDocumentClassicScriptSourceLoadCompletionAction,
        FrameDocumentClassicScriptSourceLoadRequest, FrameDocumentOwner, FrameDocumentTaskOwner,
        FrameRealmId, FrameRequestId,
    },
    parser_script::{
        action::ParserPendingClassicScriptNotification, payload::ParserClassicScriptSourceResult,
    },
    types::ChildClassicScriptLoadCompletion,
};

#[derive(Debug, Default)]
pub(crate) struct FrameParserClassicScriptRunnerStore {
    documents: BTreeMap<FrameDocumentOwner, FrameParserClassicScriptRunner>,
}

impl FrameParserClassicScriptRunnerStore {
    pub(crate) fn install_empty(&mut self, owner: FrameDocumentOwner) {
        self.documents
            .insert(owner, FrameParserClassicScriptRunner::empty(owner));
    }

    pub(crate) fn remove(&mut self, owner: FrameDocumentOwner) -> bool {
        self.documents.remove(&owner).is_some()
    }

    pub(crate) fn has_runner(&self, owner: FrameDocumentOwner) -> bool {
        self.documents.contains_key(&owner)
    }

    #[cfg(test)]
    pub(crate) fn is_complete(&self, owner: FrameDocumentOwner) -> bool {
        self.documents
            .get(&owner)
            .is_none_or(FrameParserClassicScriptRunner::is_complete)
    }

    pub(crate) fn accepts_owner_document_handle(
        &self,
        owner: FrameDocumentOwner,
        owner_document_handle: DomHandle,
    ) -> bool {
        self.documents
            .get(&owner)
            .is_some_and(|runner| runner.accepts_owner_document_handle(owner_document_handle))
    }

    pub(crate) fn push_prepared_parser_script(
        &mut self,
        owner: FrameDocumentOwner,
        owner_document_handle: DomHandle,
        pending_script: FrameParserClassicScriptItem,
    ) -> bool {
        self.documents.get_mut(&owner).is_some_and(|runner| {
            runner.push_prepared_parser_script(owner_document_handle, pending_script)
        })
    }

    pub(crate) fn next_parser_blocking_task(
        &mut self,
        owner: FrameDocumentOwner,
        child_handle: DomHandle,
        task_owner: FrameDocumentTaskOwner,
        realm_id: Option<FrameRealmId>,
        owner_current: bool,
    ) -> Option<FrameDocumentClassicScriptSchedulerWork> {
        self.documents.get_mut(&owner).and_then(|runner| {
            runner.next_parser_blocking_task(child_handle, task_owner, realm_id, owner_current)
        })
    }

    pub(crate) fn next_deferred_task(
        &mut self,
        owner: FrameDocumentOwner,
        child_handle: DomHandle,
        task_owner: FrameDocumentTaskOwner,
        realm_id: Option<FrameRealmId>,
        owner_current: bool,
    ) -> Option<FrameDocumentClassicScriptSchedulerWork> {
        self.documents.get_mut(&owner).and_then(|runner| {
            runner.next_deferred_task(child_handle, task_owner, realm_id, owner_current)
        })
    }

    pub(crate) fn current_deferred_script_key(
        &self,
        owner: FrameDocumentOwner,
    ) -> Option<crate::document_script_scheduler::ParserPendingScriptKey> {
        self.documents.get(&owner)?.current_deferred_script_key()
    }

    pub(crate) fn discard_current_deferred_script_if_key(
        &mut self,
        owner: FrameDocumentOwner,
        key: crate::document_script_scheduler::ParserPendingScriptKey,
    ) -> bool {
        self.documents
            .get_mut(&owner)
            .is_some_and(|runner| runner.discard_current_deferred_script_if_key(key))
    }

    pub(crate) fn current_parser_blocking_stylesheet_signatures(
        &self,
        owner: FrameDocumentOwner,
    ) -> Option<
        &std::collections::HashSet<crate::stylesheet_blocking::DocumentBlockingStylesheetSignature>,
    > {
        self.documents
            .get(&owner)?
            .current_parser_blocking_stylesheet_signatures()
    }

    pub(crate) fn has_current_parser_blocking_script(&self, owner: FrameDocumentOwner) -> bool {
        self.documents
            .get(&owner)
            .is_some_and(FrameParserClassicScriptRunner::has_current_parser_blocking_script)
    }

    pub(crate) fn current_deferred_stylesheet_signatures(
        &self,
        owner: FrameDocumentOwner,
    ) -> Option<
        &std::collections::HashSet<crate::stylesheet_blocking::DocumentBlockingStylesheetSignature>,
    > {
        self.documents
            .get(&owner)?
            .current_deferred_stylesheet_signatures()
    }

    pub(crate) fn begin_ready_execution(
        &mut self,
        owner: FrameDocumentOwner,
        child_handle: DomHandle,
        task_owner: FrameDocumentTaskOwner,
        realm_id: Option<FrameRealmId>,
        scheduling: FrameDocumentClassicScriptScheduling,
        pending_script_key: Option<crate::document_script_scheduler::ParserPendingScriptKey>,
        script_handle: DomHandle,
        owner_current: bool,
    ) -> Option<FrameDocumentClassicScriptBeginExecutionAction> {
        self.documents.get_mut(&owner).and_then(|runner| {
            runner.begin_ready_execution(
                child_handle,
                task_owner,
                realm_id,
                scheduling,
                pending_script_key,
                script_handle,
                owner_current,
            )
        })
    }

    pub(crate) fn dispose_ready_script(
        &mut self,
        owner: FrameDocumentOwner,
        child_handle: DomHandle,
        task_owner: FrameDocumentTaskOwner,
        realm_id: Option<FrameRealmId>,
        scheduling: FrameDocumentClassicScriptScheduling,
        pending_script_key: Option<crate::document_script_scheduler::ParserPendingScriptKey>,
        script_handle: DomHandle,
        owner_current: bool,
    ) -> Option<FrameDocumentClassicScriptCompletionAction> {
        self.documents.get_mut(&owner).and_then(|runner| {
            runner.dispose_ready_script(
                child_handle,
                task_owner,
                realm_id,
                scheduling,
                pending_script_key,
                script_handle,
                owner_current,
            )
        })
    }

    pub(crate) fn finish_executing(
        &mut self,
        owner: FrameDocumentOwner,
        child_handle: DomHandle,
        task_owner: FrameDocumentTaskOwner,
        realm_id: Option<FrameRealmId>,
        scheduling: FrameDocumentClassicScriptScheduling,
        pending_script_key: Option<crate::document_script_scheduler::ParserPendingScriptKey>,
        script_handle: DomHandle,
        owner_current: bool,
    ) -> Option<FrameDocumentClassicScriptCompletionAction> {
        self.documents.get_mut(&owner).and_then(|runner| {
            runner.finish_executing(
                child_handle,
                task_owner,
                realm_id,
                scheduling,
                pending_script_key,
                script_handle,
                owner_current,
            )
        })
    }

    pub(crate) fn source_load_client(
        &self,
        owner: FrameDocumentOwner,
        child_handle: DomHandle,
    ) -> Option<FrameDocumentClassicScriptSourceLoadClient> {
        self.documents
            .get(&owner)
            .and_then(|runner| runner.source_load_client(child_handle))
    }

    pub(crate) fn begin_external_load(
        &mut self,
        owner: FrameDocumentOwner,
        client: &FrameDocumentClassicScriptSourceLoadClient,
        load_id: u64,
        task_owner: FrameDocumentTaskOwner,
        owner_request_id: FrameRequestId,
    ) -> Option<FrameDocumentClassicScriptSourceLoadRequest> {
        self.documents.get_mut(&owner).and_then(|runner| {
            runner.begin_external_load(client, load_id, task_owner, owner_request_id)
        })
    }

    pub(crate) fn fail_external_pending_before_load(
        &mut self,
        owner: FrameDocumentOwner,
        client: &FrameDocumentClassicScriptSourceLoadClient,
        error: String,
    ) -> bool {
        self.documents
            .get_mut(&owner)
            .is_some_and(|runner| runner.fail_external_pending_before_load(client, error))
    }

    pub(crate) fn push_deferred_external_script_and_begin_load(
        &mut self,
        owner: FrameDocumentOwner,
        child_handle: DomHandle,
        pending_script: FrameParserClassicScriptItem,
        load_id: u64,
        task_owner: FrameDocumentTaskOwner,
        owner_request_id: FrameRequestId,
    ) -> Option<FrameDocumentClassicScriptSourceLoadRequest> {
        self.documents.get_mut(&owner).and_then(|runner| {
            runner.push_deferred_external_script_and_begin_load(
                child_handle,
                pending_script,
                load_id,
                task_owner,
                owner_request_id,
            )
        })
    }

    pub(crate) fn external_load_owner(
        &self,
        owner: FrameDocumentOwner,
        completion: &ChildClassicScriptLoadCompletion,
    ) -> Option<FrameDocumentClassicScriptSourceLoadCompletionAction> {
        self.documents
            .get(&owner)
            .and_then(|runner| runner.external_load_owner(completion))
    }

    pub(crate) fn notify_external_source_result(
        &mut self,
        owner: FrameDocumentOwner,
        source_result: ParserClassicScriptSourceResult,
        owner_current: bool,
    ) -> Option<ParserPendingClassicScriptNotification> {
        self.documents
            .get_mut(&owner)
            .and_then(|runner| runner.notify_external_source_result(source_result, owner_current))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame_owner_model::{DocumentId, LocalWindowId};

    fn owner(id: u64) -> FrameDocumentOwner {
        FrameDocumentOwner::new(LocalWindowId(id), DocumentId(id))
    }

    #[test]
    fn frame_parser_classic_runner_store_is_document_owner_keyed() {
        let first = owner(1);
        let second = owner(2);
        let mut store = FrameParserClassicScriptRunnerStore::default();

        assert!(store.is_complete(first));
        assert!(!store.has_runner(first));

        store.install_empty(first);
        store.install_empty(second);

        assert!(store.has_runner(first));
        assert!(store.has_runner(second));
        assert!(store.remove(first));
        assert!(!store.has_runner(first));
        assert!(store.has_runner(second));
    }
}
