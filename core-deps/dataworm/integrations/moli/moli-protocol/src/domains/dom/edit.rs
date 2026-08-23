use chromiumoxide_cdp::cdp::browser_protocol::dom::{
    MoveToParams, SetAttributesAsTextParams, SetNodeNameParams, SetNodeValueParams,
    SetOuterHtmlParams,
};
use moli_core::page::RendererDomEdit;

use super::resolve::PendingDomCommandStartError;
use crate::{conn::Cmd, domains::actions::DomAction};

fn frontend_node_id(value: i64) -> Result<u32, PendingDomCommandStartError> {
    u32::try_from(value)
        .ok()
        .filter(|value| *value != 0)
        .ok_or_else(PendingDomCommandStartError::invalid_params)
}

pub(super) fn renderer_dom_edit_from_cdp(
    cmd: &Cmd<'_>,
    action: DomAction,
) -> Result<RendererDomEdit, PendingDomCommandStartError> {
    match action {
        DomAction::MoveTo => {
            let params: MoveToParams = cmd
                .get_params()
                .ok()
                .flatten()
                .ok_or_else(PendingDomCommandStartError::invalid_params)?;
            Ok(RendererDomEdit::MoveTo {
                node_id: frontend_node_id(*params.node_id.inner())?,
                target_node_id: frontend_node_id(*params.target_node_id.inner())?,
                insert_before_node_id: params
                    .insert_before_node_id
                    .filter(|node_id| *node_id.inner() != 0)
                    .map(|node_id| frontend_node_id(*node_id.inner()))
                    .transpose()?,
            })
        }
        DomAction::SetAttributesAsText => {
            let params: SetAttributesAsTextParams = cmd
                .get_params()
                .ok()
                .flatten()
                .ok_or_else(PendingDomCommandStartError::invalid_params)?;
            Ok(RendererDomEdit::SetAttributesAsText {
                node_id: frontend_node_id(*params.node_id.inner())?,
                text: params.text,
                name: params.name,
            })
        }
        DomAction::SetNodeName => {
            let params: SetNodeNameParams = cmd
                .get_params()
                .ok()
                .flatten()
                .ok_or_else(PendingDomCommandStartError::invalid_params)?;
            Ok(RendererDomEdit::SetNodeName {
                node_id: frontend_node_id(*params.node_id.inner())?,
                name: params.name,
            })
        }
        DomAction::SetNodeValue => {
            let params: SetNodeValueParams = cmd
                .get_params()
                .ok()
                .flatten()
                .ok_or_else(PendingDomCommandStartError::invalid_params)?;
            Ok(RendererDomEdit::SetNodeValue {
                node_id: frontend_node_id(*params.node_id.inner())?,
                value: params.value,
            })
        }
        DomAction::SetOuterHtml => {
            let params: SetOuterHtmlParams = cmd
                .get_params()
                .ok()
                .flatten()
                .ok_or_else(PendingDomCommandStartError::invalid_params)?;
            Ok(RendererDomEdit::SetOuterHtml {
                node_id: frontend_node_id(*params.node_id.inner())?,
                outer_html: params.outer_html,
            })
        }
        _ => unreachable!("DOM edit parser requires a DOM edit action"),
    }
}
