use crate::{
    document_runtime::DomHandle,
    document_script_scheduler::{
        FrameParserClassicScriptItem,
        external_pending_frame_parser_classic_script_item_with_blocking_signatures,
        inline_frame_parser_classic_script_item_with_blocking_signatures,
    },
    dom::{NodeId, native::DomHost},
    frame_owner_model::{
        DocumentLoadDelayTokenId, FrameDocumentClassicScriptScheduling, FrameDocumentTaskOwner,
    },
    parser_script::payload::{ParserClassicScriptMetadata, ParserPreparedClassicScript},
    planning::{PreparedScript, ScriptSource},
};
use std::collections::HashSet;

#[derive(Debug, Clone)]
pub(in crate::native_bridge::context_host) struct ChildParserClassicScriptCandidate {
    input: ParserPreparedClassicScript,
    scheduling: FrameDocumentClassicScriptScheduling,
    blocking_stylesheet_signatures:
        HashSet<crate::stylesheet_blocking::DocumentBlockingStylesheetSignature>,
}

impl ChildParserClassicScriptCandidate {
    pub(in crate::native_bridge::context_host) fn from_parser_handoff(
        script_handle: DomHandle,
        start_line: u64,
        blocking_stylesheet_signatures: HashSet<
            crate::stylesheet_blocking::DocumentBlockingStylesheetSignature,
        >,
        mut script: PreparedScript,
    ) -> Self {
        script.node_id = NodeId::new(script_handle.index());
        Self {
            input: ParserPreparedClassicScript::new(
                ParserClassicScriptMetadata::new(script_handle, start_line),
                script,
            ),
            scheduling: FrameDocumentClassicScriptScheduling::ParserBlocking,
            blocking_stylesheet_signatures,
        }
    }

    pub(in crate::native_bridge::context_host) fn from_deferred_handoff(
        script_handle: DomHandle,
        start_line: u64,
        blocking_stylesheet_signatures: HashSet<
            crate::stylesheet_blocking::DocumentBlockingStylesheetSignature,
        >,
        mut script: PreparedScript,
    ) -> Self {
        script.node_id = NodeId::new(script_handle.index());
        Self {
            input: ParserPreparedClassicScript::new(
                ParserClassicScriptMetadata::new(script_handle, start_line),
                script,
            ),
            scheduling: FrameDocumentClassicScriptScheduling::Deferred,
            blocking_stylesheet_signatures,
        }
    }

    pub(super) fn scheduling(&self) -> FrameDocumentClassicScriptScheduling {
        self.scheduling
    }

    pub(super) fn pending_script_key(
        &self,
    ) -> crate::document_script_scheduler::ParserPendingScriptKey {
        crate::document_script_scheduler::ParserPendingScriptKey::from_script(self.input.script())
    }
}

struct ChildParserClassicScriptPreparation;

impl ChildParserClassicScriptPreparation {
    fn prepare_script(
        &self,
        dom_host: &mut DomHost,
        task_owner: FrameDocumentTaskOwner,
        owner_document_handle: DomHandle,
        candidate: ChildParserClassicScriptCandidate,
        load_delay_token: Option<DocumentLoadDelayTokenId>,
    ) -> Option<FrameParserClassicScriptItem> {
        let ChildParserClassicScriptCandidate {
            input,
            scheduling: _,
            blocking_stylesheet_signatures,
        } = candidate;
        self.prepare_prepared_script(
            dom_host,
            task_owner,
            owner_document_handle,
            input,
            blocking_stylesheet_signatures,
            load_delay_token,
        )
    }

    fn prepare_prepared_script(
        &self,
        dom_host: &mut DomHost,
        task_owner: FrameDocumentTaskOwner,
        owner_document_handle: DomHandle,
        input: ParserPreparedClassicScript,
        blocking_stylesheet_signatures: HashSet<
            crate::stylesheet_blocking::DocumentBlockingStylesheetSignature,
        >,
        load_delay_token: Option<DocumentLoadDelayTokenId>,
    ) -> Option<FrameParserClassicScriptItem> {
        let script_handle = input.metadata().script_handle();
        let _ = dom_host.set_script_already_started(script_handle, true);
        match &input.script().source {
            ScriptSource::Inline(_) => Some(
                inline_frame_parser_classic_script_item_with_blocking_signatures(
                    input,
                    task_owner,
                    owner_document_handle,
                    blocking_stylesheet_signatures,
                    load_delay_token,
                ),
            ),
            ScriptSource::External => Some(
                external_pending_frame_parser_classic_script_item_with_blocking_signatures(
                    input,
                    task_owner,
                    owner_document_handle,
                    blocking_stylesheet_signatures,
                    load_delay_token,
                ),
            ),
            ScriptSource::Loaded(_) | ScriptSource::LoadedBinary { .. } => None,
        }
    }
}

pub(super) fn prepare_child_parser_classic_script(
    dom_host: &mut DomHost,
    task_owner: FrameDocumentTaskOwner,
    owner_document_handle: DomHandle,
    parser_script: ChildParserClassicScriptCandidate,
    load_delay_token: Option<DocumentLoadDelayTokenId>,
) -> Option<FrameParserClassicScriptItem> {
    ChildParserClassicScriptPreparation.prepare_script(
        dom_host,
        task_owner,
        owner_document_handle,
        parser_script,
        load_delay_token,
    )
}
