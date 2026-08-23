use super::super::{callback_arg_dom_handle, callback_arg_string, runtime_ptr_from_object};

pub(in crate::native_bridge) fn bridge_set_input_value_callback(
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
    let runtime = unsafe { &mut *runtime_ptr };
    rv.set(v8::Boolean::new(scope, runtime.set_input_value(handle, &value)).into());
}

pub(in crate::native_bridge) fn bridge_set_checked_state_callback(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments<'_>,
    mut rv: v8::ReturnValue<'_, v8::Value>,
) {
    let Some(handle) = callback_arg_dom_handle(scope, &args, 0) else {
        rv.set(v8::Boolean::new(scope, false).into());
        return;
    };
    let checked = args.get(1).boolean_value(scope);
    let bridge = args.this();
    let Ok(runtime_ptr) = runtime_ptr_from_object(scope, bridge) else {
        rv.set(v8::Boolean::new(scope, false).into());
        return;
    };
    let runtime = unsafe { &mut *runtime_ptr };
    let did_change = runtime.set_checked_state(scope, runtime_ptr, handle, checked);
    rv.set(v8::Boolean::new(scope, did_change).into());
}

pub(in crate::native_bridge) fn bridge_set_selected_state_callback(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments<'_>,
    mut rv: v8::ReturnValue<'_, v8::Value>,
) {
    let Some(handle) = callback_arg_dom_handle(scope, &args, 0) else {
        rv.set(v8::Boolean::new(scope, false).into());
        return;
    };
    let selected = args.get(1).boolean_value(scope);
    let bridge = args.this();
    let Ok(runtime_ptr) = runtime_ptr_from_object(scope, bridge) else {
        rv.set(v8::Boolean::new(scope, false).into());
        return;
    };
    let runtime = unsafe { &mut *runtime_ptr };
    let did_change = runtime.set_selected_state(scope, runtime_ptr, handle, selected);
    rv.set(v8::Boolean::new(scope, did_change).into());
}

pub(in crate::native_bridge) fn bridge_set_indeterminate_state_callback(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments<'_>,
    mut rv: v8::ReturnValue<'_, v8::Value>,
) {
    let Some(handle) = callback_arg_dom_handle(scope, &args, 0) else {
        rv.set(v8::Boolean::new(scope, false).into());
        return;
    };
    let indeterminate = args.get(1).boolean_value(scope);
    let bridge = args.this();
    let Ok(runtime_ptr) = runtime_ptr_from_object(scope, bridge) else {
        rv.set(v8::Boolean::new(scope, false).into());
        return;
    };
    let runtime = unsafe { &mut *runtime_ptr };
    let did_change = runtime.set_indeterminate_state(scope, runtime_ptr, handle, indeterminate);
    rv.set(v8::Boolean::new(scope, did_change).into());
}
