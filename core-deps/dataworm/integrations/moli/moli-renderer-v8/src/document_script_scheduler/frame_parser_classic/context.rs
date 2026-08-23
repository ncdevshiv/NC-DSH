use crate::document_runtime::DomHandle;
use crate::document_script_scheduler::ParserPendingScriptKey;
use crate::frame_owner_model::{
    DocumentLoadDelayTokenId, FrameDocumentClassicScriptSourceLoadOwner, FrameDocumentTaskOwner,
};
use crate::parser_script::context::{
    ParserClassicScriptDocumentOwnerState, ParserClassicScriptExecutionGateState,
    ParserClassicScriptSourceLoadCompletionState, ParserClassicScriptSourceLoadStartState,
    ParserClassicScriptSourceLoadState,
};
use std::collections::HashSet;

#[derive(Debug, Clone)]
pub(crate) struct FrameParserClassicScriptContext {
    task_owner: FrameDocumentTaskOwner,
    owner_document_handle: DomHandle,
    pending_script_key: ParserPendingScriptKey,
    blocking_stylesheet_signatures:
        HashSet<crate::stylesheet_blocking::DocumentBlockingStylesheetSignature>,
    load_delay_token: Option<DocumentLoadDelayTokenId>,
    pub(super) source_load_owner: Option<FrameDocumentClassicScriptSourceLoadOwner>,
}

impl FrameParserClassicScriptContext {
    #[cfg(test)]
    pub(super) fn new(
        task_owner: FrameDocumentTaskOwner,
        owner_document_handle: DomHandle,
        pending_script_key: ParserPendingScriptKey,
    ) -> Self {
        Self::with_blocking_stylesheet_signatures(
            task_owner,
            owner_document_handle,
            pending_script_key,
            HashSet::new(),
            None,
        )
    }

    pub(super) fn with_blocking_stylesheet_signatures(
        task_owner: FrameDocumentTaskOwner,
        owner_document_handle: DomHandle,
        pending_script_key: ParserPendingScriptKey,
        blocking_stylesheet_signatures: HashSet<
            crate::stylesheet_blocking::DocumentBlockingStylesheetSignature,
        >,
        load_delay_token: Option<DocumentLoadDelayTokenId>,
    ) -> Self {
        Self {
            task_owner,
            owner_document_handle,
            pending_script_key,
            blocking_stylesheet_signatures,
            load_delay_token,
            source_load_owner: None,
        }
    }

    pub(super) fn owner_document_handle(&self) -> DomHandle {
        self.owner_document_handle
    }

    pub(super) fn pending_script_key(&self) -> ParserPendingScriptKey {
        self.pending_script_key
    }

    pub(super) fn blocking_stylesheet_signatures(
        &self,
    ) -> &HashSet<crate::stylesheet_blocking::DocumentBlockingStylesheetSignature> {
        &self.blocking_stylesheet_signatures
    }

    pub(super) fn load_delay_token(&self) -> Option<DocumentLoadDelayTokenId> {
        self.load_delay_token
    }
}

impl ParserClassicScriptDocumentOwnerState for FrameParserClassicScriptContext {
    fn parser_classic_document_task_owner(&self) -> FrameDocumentTaskOwner {
        self.task_owner
    }
}

impl ParserClassicScriptExecutionGateState for FrameParserClassicScriptContext {
    type ExecutionGateState = ();

    fn parser_classic_execution_gate_state(&self) -> Self::ExecutionGateState {}
}

impl ParserClassicScriptSourceLoadState for FrameParserClassicScriptContext {
    fn clear_parser_classic_source_load_state(&mut self) {
        self.source_load_owner = None;
    }
}

impl ParserClassicScriptSourceLoadStartState for FrameParserClassicScriptContext {
    type SourceLoadState = FrameDocumentClassicScriptSourceLoadOwner;

    fn install_parser_classic_source_load_state(&mut self, state: Self::SourceLoadState) {
        self.source_load_owner = Some(state);
    }
}

impl ParserClassicScriptSourceLoadCompletionState for FrameParserClassicScriptContext {
    type SourceLoadOwner = FrameDocumentClassicScriptSourceLoadOwner;

    fn parser_classic_source_load_owner(&self) -> Option<Self::SourceLoadOwner> {
        self.source_load_owner
    }
}
