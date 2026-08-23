use super::super::range_algorithms::point_order;
use super::*;
use crate::webidl;

#[derive(webidl::WebIdlArgs)]
#[webidl(prefix = "Range boundary method")]
struct RangeRelativeBoundaryArgs<'s> {
    #[webidl(with = range_relative_boundary_node_arg)]
    node: v8::Local<'s, v8::Object>,
}

fn range_relative_boundary_node_arg<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: &v8::FunctionCallbackArguments<'s>,
    index: i32,
) -> Result<v8::Local<'s, v8::Object>, webidl::WebIdlError> {
    webidl_node_arg(scope, args, index, "Range boundary method requires a Node")
}

pub(in crate::context_bootstrap) fn range_set_boundary_relative<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue<'_, v8::Value>,
    is_start: bool,
    after: bool,
) {
    let Some(parsed) = webidl::parse_args::<RangeRelativeBoundaryArgs<'s>>(scope, &args) else {
        return;
    };
    let node = parsed.node;
    // Spec setStartBefore/setStartAfter/setEndBefore/setEndAfter(node):
    // If node has no parent, throw InvalidNodeTypeError (the node is not a
    // child of any container, so before/after has no boundary point). WPT
    // tests this explicitly with detached / document-root nodes.
    let Some(parent) = object_property_as_object(scope, node, "parentNode") else {
        super::exceptions::throw_named_dom_exception(
            scope,
            "InvalidNodeTypeError",
            "Range boundary method requires a node with a parent.",
        );
        return;
    };
    let Some(index) = child_index(scope, parent, node) else {
        rv.set_undefined();
        return;
    };
    let offset = index + u32::from(after);
    if is_start {
        set_range_boundary(scope, args.this(), RangeBoundarySide::Start, parent, offset);
        let Some(end_container) =
            range_boundary_container_object(scope, args.this(), RangeBoundarySide::End)
        else {
            rv.set_undefined();
            return;
        };
        let end_offset = range_boundary_offset(scope, args.this(), RangeBoundarySide::End) as u32;
        if point_order(scope, parent, offset, end_container, end_offset)
            .is_none_or(|order| order == std::cmp::Ordering::Greater)
        {
            set_range_boundary(scope, args.this(), RangeBoundarySide::End, parent, offset);
        }
    } else {
        set_range_boundary(scope, args.this(), RangeBoundarySide::End, parent, offset);
        let Some(start_container) =
            range_boundary_container_object(scope, args.this(), RangeBoundarySide::Start)
        else {
            rv.set_undefined();
            return;
        };
        let start_offset =
            range_boundary_offset(scope, args.this(), RangeBoundarySide::Start) as u32;
        if point_order(scope, start_container, start_offset, parent, offset)
            .is_none_or(|order| order == std::cmp::Ordering::Greater)
        {
            set_range_boundary(scope, args.this(), RangeBoundarySide::Start, parent, offset);
        }
    }
    rv.set_undefined();
}
