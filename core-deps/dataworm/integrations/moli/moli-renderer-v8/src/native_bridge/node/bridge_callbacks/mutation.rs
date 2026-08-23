use super::*;

pub(in crate::native_bridge) fn bridge_append_child_callback(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments<'_>,
    mut rv: v8::ReturnValue<'_, v8::Value>,
) {
    let Some(parent) = callback_arg_dom_handle(scope, &args, 0) else {
        rv.set_null();
        return;
    };
    let Some(child) = callback_arg_dom_handle(scope, &args, 1) else {
        rv.set_null();
        return;
    };
    let bridge = args.this();
    let Ok(runtime_ptr) = runtime_ptr_from_object(scope, bridge) else {
        rv.set_null();
        return;
    };
    if !append_child_in_reaction_scope(scope, runtime_ptr, parent, child) {
        rv.set_null();
        return;
    }
    set_wrapped_handle_or_null(scope, &mut rv, runtime_ptr, Some(child));
}

pub(in crate::native_bridge) fn bridge_remove_child_callback(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments<'_>,
    mut rv: v8::ReturnValue<'_, v8::Value>,
) {
    let Some(parent) = callback_arg_dom_handle(scope, &args, 0) else {
        rv.set_null();
        return;
    };
    let Some(child) = callback_arg_dom_handle(scope, &args, 1) else {
        rv.set_null();
        return;
    };
    let bridge = args.this();
    let Ok(runtime_ptr) = runtime_ptr_from_object(scope, bridge) else {
        rv.set_null();
        return;
    };
    if !remove_child_in_reaction_scope(scope, runtime_ptr, parent, child) {
        rv.set_null();
        return;
    }
    set_wrapped_handle_or_null(scope, &mut rv, runtime_ptr, Some(child));
}

pub(in crate::native_bridge) fn bridge_insert_before_callback(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments<'_>,
    mut rv: v8::ReturnValue<'_, v8::Value>,
) {
    let Some(parent) = callback_arg_dom_handle(scope, &args, 0) else {
        rv.set_null();
        return;
    };
    let Some(child) = callback_arg_dom_handle(scope, &args, 1) else {
        rv.set_null();
        return;
    };
    let reference = callback_arg_dom_handle(scope, &args, 2);
    let bridge = args.this();
    let Ok(runtime_ptr) = runtime_ptr_from_object(scope, bridge) else {
        rv.set_null();
        return;
    };
    if !insert_before_in_reaction_scope(scope, runtime_ptr, parent, child, reference) {
        rv.set_null();
        return;
    }
    set_wrapped_handle_or_null(scope, &mut rv, runtime_ptr, Some(child));
}

pub(in crate::native_bridge) fn bridge_set_text_content_callback(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments<'_>,
    mut rv: v8::ReturnValue<'_, v8::Value>,
) {
    let Some(handle) = callback_arg_dom_handle(scope, &args, 0) else {
        rv.set(v8::Boolean::new(scope, false).into());
        return;
    };
    let Some(value) = callback_arg_string(scope, &args, 1) else {
        rv.set(v8::Boolean::new(scope, false).into());
        return;
    };
    let bridge = args.this();
    let Ok(runtime_ptr) = runtime_ptr_from_object(scope, bridge) else {
        rv.set(v8::Boolean::new(scope, false).into());
        return;
    };
    let did_set = set_text_content_in_reaction_scope(scope, runtime_ptr, handle, &value);
    rv.set(v8::Boolean::new(scope, did_set).into());
}
