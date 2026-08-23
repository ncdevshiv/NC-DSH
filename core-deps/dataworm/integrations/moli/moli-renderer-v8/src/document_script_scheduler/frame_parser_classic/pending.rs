use super::context::FrameParserClassicScriptContext;
use crate::{
    document_runtime::DomHandle,
    frame_owner_model::FrameDocumentTaskOwner,
    parser_script::{item::ParserClassicScriptRunnerItem, payload::ParserPreparedClassicScript},
};
use std::collections::HashSet;

pub(crate) type FrameParserClassicScriptItem =
    ParserClassicScriptRunnerItem<FrameParserClassicScriptContext>;

#[cfg(test)]
pub(crate) fn inline_frame_parser_classic_script_item(
    input: ParserPreparedClassicScript,
    task_owner: FrameDocumentTaskOwner,
    owner_document_handle: DomHandle,
) -> FrameParserClassicScriptItem {
    inline_frame_parser_classic_script_item_with_blocking_signatures(
        input,
        task_owner,
        owner_document_handle,
        HashSet::new(),
        None,
    )
}

pub(crate) fn inline_frame_parser_classic_script_item_with_blocking_signatures(
    input: ParserPreparedClassicScript,
    task_owner: FrameDocumentTaskOwner,
    owner_document_handle: DomHandle,
    blocking_stylesheet_signatures: HashSet<
        crate::stylesheet_blocking::DocumentBlockingStylesheetSignature,
    >,
    load_delay_token: Option<crate::frame_owner_model::DocumentLoadDelayTokenId>,
) -> FrameParserClassicScriptItem {
    let pending_script_key =
        crate::document_script_scheduler::ParserPendingScriptKey::from_script(input.script());
    ParserClassicScriptRunnerItem::inline_ready(
        input,
        FrameParserClassicScriptContext::with_blocking_stylesheet_signatures(
            task_owner,
            owner_document_handle,
            pending_script_key,
            blocking_stylesheet_signatures,
            load_delay_token,
        ),
    )
}

#[cfg(test)]
pub(crate) fn external_pending_frame_parser_classic_script_item(
    input: ParserPreparedClassicScript,
    task_owner: FrameDocumentTaskOwner,
    owner_document_handle: DomHandle,
) -> FrameParserClassicScriptItem {
    external_pending_frame_parser_classic_script_item_with_blocking_signatures(
        input,
        task_owner,
        owner_document_handle,
        HashSet::new(),
        None,
    )
}

pub(crate) fn external_pending_frame_parser_classic_script_item_with_blocking_signatures(
    input: ParserPreparedClassicScript,
    task_owner: FrameDocumentTaskOwner,
    owner_document_handle: DomHandle,
    blocking_stylesheet_signatures: HashSet<
        crate::stylesheet_blocking::DocumentBlockingStylesheetSignature,
    >,
    load_delay_token: Option<crate::frame_owner_model::DocumentLoadDelayTokenId>,
) -> FrameParserClassicScriptItem {
    let pending_script_key =
        crate::document_script_scheduler::ParserPendingScriptKey::from_script(input.script());
    ParserClassicScriptRunnerItem::external_pending(
        input,
        FrameParserClassicScriptContext::with_blocking_stylesheet_signatures(
            task_owner,
            owner_document_handle,
            pending_script_key,
            blocking_stylesheet_signatures,
            load_delay_token,
        ),
    )
}

#[cfg(test)]
#[path = "pending_tests.rs"]
mod tests;
