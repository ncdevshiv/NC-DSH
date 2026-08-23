use crate::util::v8_string;

use super::super::super::{callback_arg_dom_handle, callback_arg_string, runtime_ptr_from_object};
use super::super::{element_attribute, update_iframe_snapshot_navigation};

pub(in crate::native_bridge) fn bridge_get_attribute_callback(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments<'_>,
    mut rv: v8::ReturnValue<'_, v8::Value>,
) {
    let Some(handle) = callback_arg_dom_handle(scope, &args, 0) else {
        rv.set_null();
        return;
    };
    let Some(name) = callback_arg_string(scope, &args, 1) else {
        rv.set_null();
        return;
    };
    let bridge = args.this();
    let Ok(runtime_ptr) = runtime_ptr_from_object(scope, bridge) else {
        rv.set_null();
        return;
    };
    let runtime = unsafe { &*runtime_ptr };
    let Some(value) = element_attribute(runtime, handle, &name) else {
        rv.set_null();
        return;
    };
    let Some(value) = v8_string(scope, &value) else {
        rv.set_null();
        return;
    };
    rv.set(value.into());
}

pub(in crate::native_bridge) fn bridge_set_attribute_callback(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments<'_>,
    mut rv: v8::ReturnValue<'_, v8::Value>,
) {
    let Some(handle) = callback_arg_dom_handle(scope, &args, 0) else {
        rv.set(v8::Boolean::new(scope, false).into());
        return;
    };
    let Some(name) = callback_arg_string(scope, &args, 1) else {
        rv.set(v8::Boolean::new(scope, false).into());
        return;
    };
    let Some(value) = callback_arg_string(scope, &args, 2) else {
        rv.set(v8::Boolean::new(scope, false).into());
        return;
    };
    let bridge = args.this();
    let Ok(runtime_ptr) = runtime_ptr_from_object(scope, bridge) else {
        rv.set(v8::Boolean::new(scope, false).into());
        return;
    };
    let runtime = unsafe { &mut *runtime_ptr };
    if name.eq_ignore_ascii_case("src")
        && runtime.dom_host().is_html_element_named(handle, "iframe")
    {
        update_iframe_snapshot_navigation(scope, runtime_ptr, handle, &value);
        rv.set(v8::Boolean::new(scope, true).into());
        return;
    }
    let did_set = runtime.set_attribute(scope, runtime_ptr, handle, &name, &value);
    if did_set && name.eq_ignore_ascii_case("style") {
        runtime.set_element_inline_style_current_base_url(handle);
    }
    crate::context_bootstrap::reset_html_canvas_backing_store_for_dimension_assignment(
        scope,
        runtime_ptr,
        handle,
        None,
        &name,
    );
    rv.set(v8::Boolean::new(scope, did_set).into());
}

pub(in crate::native_bridge) fn bridge_remove_attribute_callback(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments<'_>,
    mut rv: v8::ReturnValue<'_, v8::Value>,
) {
    let Some(handle) = callback_arg_dom_handle(scope, &args, 0) else {
        rv.set(v8::Boolean::new(scope, false).into());
        return;
    };
    let Some(name) = callback_arg_string(scope, &args, 1) else {
        rv.set(v8::Boolean::new(scope, false).into());
        return;
    };
    let bridge = args.this();
    let Ok(runtime_ptr) = runtime_ptr_from_object(scope, bridge) else {
        rv.set(v8::Boolean::new(scope, false).into());
        return;
    };
    let runtime = unsafe { &mut *runtime_ptr };
    let did_remove = runtime.remove_attribute(scope, runtime_ptr, handle, &name);
    if did_remove {
        crate::context_bootstrap::reset_html_canvas_backing_store_for_dimension_assignment(
            scope,
            runtime_ptr,
            handle,
            None,
            &name,
        );
    }
    rv.set(v8::Boolean::new(scope, did_remove).into());
}
