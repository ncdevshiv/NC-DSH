use super::lookup::bridge_handle_lookup_callback;
use super::*;

pub(in crate::native_bridge) fn bridge_owner_document_callback(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments<'_>,
    rv: v8::ReturnValue<'_, v8::Value>,
) {
    bridge_handle_lookup_callback(scope, args, rv, |runtime, handle| {
        runtime
            .dom_host()
            .node(handle)
            .and_then(Node::owner_document)
    });
}

pub(in crate::native_bridge) fn bridge_parent_node_callback(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments<'_>,
    rv: v8::ReturnValue<'_, v8::Value>,
) {
    bridge_handle_lookup_callback(scope, args, rv, |runtime, handle| {
        runtime.dom_host().node(handle).and_then(Node::parent_node)
    });
}

pub(in crate::native_bridge) fn bridge_first_child_callback(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments<'_>,
    rv: v8::ReturnValue<'_, v8::Value>,
) {
    bridge_handle_lookup_callback(scope, args, rv, |runtime, handle| {
        runtime.dom_host().node(handle).and_then(Node::first_child)
    });
}

pub(in crate::native_bridge) fn bridge_last_child_callback(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments<'_>,
    rv: v8::ReturnValue<'_, v8::Value>,
) {
    bridge_handle_lookup_callback(scope, args, rv, |runtime, handle| {
        runtime.dom_host().node(handle).and_then(Node::last_child)
    });
}

pub(in crate::native_bridge) fn bridge_next_sibling_callback(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments<'_>,
    rv: v8::ReturnValue<'_, v8::Value>,
) {
    bridge_handle_lookup_callback(scope, args, rv, |runtime, handle| {
        runtime.dom_host().node(handle).and_then(Node::next_sibling)
    });
}

pub(in crate::native_bridge) fn bridge_previous_sibling_callback(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments<'_>,
    rv: v8::ReturnValue<'_, v8::Value>,
) {
    bridge_handle_lookup_callback(scope, args, rv, |runtime, handle| {
        runtime.dom_host().node(handle).and_then(Node::prev_sibling)
    });
}
