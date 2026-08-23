use super::*;
use crate::webidl;

#[derive(webidl::WebIdlArgs)]
#[webidl(prefix = "Selection.modify")]
struct SelectionModifyArgs {
    #[webidl(default = "")]
    action: String,
    #[webidl(default = "")]
    direction: String,
    #[webidl(default = "")]
    granularity: String,
}

pub(in crate::context_bootstrap) fn selection_delete_from_document_callback<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    _rv: v8::ReturnValue<'_, v8::Value>,
) {
    let Some(range) = selection_range(scope, args.this()) else {
        return;
    };
    if range_is_collapsed(scope, range) {
        return;
    }
    if range_delete_contents(scope, range).is_none() {
        return;
    }
    let Some(container) = range_boundary_container_object(scope, range, RangeBoundarySide::Start)
    else {
        return;
    };
    let offset = range_boundary_offset(scope, range, RangeBoundarySide::Start) as u32;
    selection_store(
        scope,
        args.this(),
        range,
        container,
        offset,
        container,
        offset,
        "none",
    );
    selection_dispatch_change(scope);
}

pub(in crate::context_bootstrap) fn selection_modify_callback<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    _rv: v8::ReturnValue<'_, v8::Value>,
) {
    let Some(parsed) = webidl::parse_args::<SelectionModifyArgs>(scope, &args) else {
        return;
    };
    match parsed.action.as_str() {
        "move" => {
            let Some(focus_node) = selection_focus_node(scope, args.this()) else {
                return;
            };
            let focus_offset = selection_focus_offset(scope, args.this());
            let Some((next_node, next_offset)) = selection_modify_target(
                scope,
                focus_node,
                focus_offset,
                &parsed.direction,
                &parsed.granularity,
            ) else {
                return;
            };
            if selection_set_collapsed(scope, args.this(), next_node, next_offset) {
                selection_dispatch_change(scope);
            }
        }
        "extend" => {
            let Some(anchor_node) = selection_anchor_node(scope, args.this()) else {
                return;
            };
            let anchor_offset = selection_anchor_offset(scope, args.this());
            let Some(focus_node) = selection_focus_node(scope, args.this()) else {
                return;
            };
            let focus_offset = selection_focus_offset(scope, args.this());
            let Some((next_focus_node, next_focus_offset)) = selection_modify_target(
                scope,
                focus_node,
                focus_offset,
                &parsed.direction,
                &parsed.granularity,
            ) else {
                return;
            };
            let Some(document) = node_owner_document_or_self(scope, anchor_node) else {
                return;
            };
            let Some(range) = new_range_for_document(scope, document) else {
                return;
            };
            let direction = match boundary_order(
                scope,
                anchor_node,
                anchor_offset,
                next_focus_node,
                next_focus_offset,
            ) {
                std::cmp::Ordering::Less => "forward",
                std::cmp::Ordering::Greater => "backward",
                std::cmp::Ordering::Equal => "none",
            };
            selection_store(
                scope,
                args.this(),
                range,
                anchor_node,
                anchor_offset,
                next_focus_node,
                next_focus_offset,
                direction,
            );
            selection_dispatch_change(scope);
        }
        _ => {}
    }
}
