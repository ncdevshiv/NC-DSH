use super::super::node::{node_is_element, node_runtime_and_handle_from_args_or_detached};
use super::super::{throw_dom_exception, webidl_long_from_number};

fn pointer_id_arg(
    scope: &mut v8::PinScope<'_, '_>,
    args: &v8::FunctionCallbackArguments<'_>,
) -> i32 {
    let value = args.get(0);
    let number = value.number_value(scope).unwrap_or(0.0);
    webidl_long_from_number(number)
}

pub(super) fn node_set_pointer_capture_callback<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    _rv: v8::ReturnValue<'_, v8::Value>,
) {
    let Ok((runtime_ptr, handle)) = node_runtime_and_handle_from_args_or_detached(scope, &args)
    else {
        return;
    };
    let pointer_id = pointer_id_arg(scope, &args);
    let runtime = unsafe { &mut *runtime_ptr };
    if !node_is_element(runtime, handle) {
        return;
    }
    if !pointer_capture_receiver_has_frame(scope, runtime, runtime_ptr, handle, args.this()) {
        return;
    }
    if !runtime.pointer_capture_is_active(pointer_id) {
        throw_dom_exception(
            scope,
            "NotFoundError",
            8,
            "No active pointer with the given id is found.",
        );
        return;
    }
    if !runtime.dom_host().is_connected(handle) {
        throw_dom_exception(scope, "InvalidStateError", 11, "InvalidStateError");
        return;
    }
    runtime.set_pending_pointer_capture_target(pointer_id, handle);
}

pub(super) fn node_release_pointer_capture_callback<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    _rv: v8::ReturnValue<'_, v8::Value>,
) {
    let Ok((runtime_ptr, handle)) = node_runtime_and_handle_from_args_or_detached(scope, &args)
    else {
        return;
    };
    let pointer_id = pointer_id_arg(scope, &args);
    let runtime = unsafe { &mut *runtime_ptr };
    if !node_is_element(runtime, handle) {
        return;
    }
    if !pointer_capture_receiver_has_frame(scope, runtime, runtime_ptr, handle, args.this()) {
        return;
    }
    if !runtime.pointer_capture_is_active(pointer_id) {
        throw_dom_exception(
            scope,
            "NotFoundError",
            8,
            "No active pointer with the given id is found.",
        );
        return;
    }
    if runtime.has_pending_pointer_capture_target(pointer_id, handle) {
        runtime.release_pending_pointer_capture_target(pointer_id);
    }
}

pub(super) fn node_has_pointer_capture_callback<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue<'_, v8::Value>,
) {
    let Ok((runtime_ptr, handle)) = node_runtime_and_handle_from_args_or_detached(scope, &args)
    else {
        rv.set_bool(false);
        return;
    };
    let pointer_id = pointer_id_arg(scope, &args);
    let runtime = unsafe { &*runtime_ptr };
    if !pointer_capture_receiver_has_frame(scope, runtime, runtime_ptr, handle, args.this()) {
        rv.set_bool(false);
        return;
    }
    let has_capture =
        node_is_element(runtime, handle) && runtime.has_pointer_capture_target(pointer_id, handle);
    rv.set_bool(has_capture);
}

fn pointer_capture_receiver_has_frame<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    runtime: &super::super::JsContextHost,
    runtime_ptr: *mut super::super::JsContextHost,
    handle: crate::document_runtime::DomHandle,
    receiver: v8::Local<'s, v8::Object>,
) -> bool {
    if super::super::document::detached_native_handle_for_runtime(scope, runtime_ptr, receiver)
        .is_none()
    {
        return true;
    }
    runtime.dom_host().owner_document_handle(handle) == Some(runtime.dom_host().document_handle())
}
