use crate::util::v8_string;

use super::super::super::document::set_detached_text_replacement_value;
use super::super::super::node::{
    node_runtime_and_handle_from_args_or_detached, node_runtime_and_handle_from_object,
    node_runtime_and_handle_from_object_or_detached, set_text_content_in_reaction_scope,
    throw_incompatible_method_receiver,
};
use super::super::{property_dom_string_value, resolve_url_like_attribute};

pub(in crate::native_bridge) fn anchor_text_getter_function<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue<'_, v8::Value>,
) {
    let Ok((runtime_ptr, handle)) =
        node_runtime_and_handle_from_object_or_detached(scope, args.this())
    else {
        rv.set_empty_string();
        return;
    };
    let value = unsafe { &*runtime_ptr }
        .dom_host()
        .text_content(handle)
        .unwrap_or_default();
    if let Some(value) = v8_string(scope, &value) {
        rv.set(value.into());
    } else {
        rv.set_empty_string();
    }
}

pub(in crate::native_bridge) fn anchor_text_setter_function<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue<'_, v8::Value>,
) {
    let Some(value) = property_dom_string_value(scope, args.get(0), "HTMLAnchorElement", "text")
    else {
        return;
    };
    if set_detached_text_replacement_value(scope, args.this(), &value).is_some() {
        rv.set_undefined();
        return;
    }
    let Ok((runtime_ptr, handle)) = node_runtime_and_handle_from_object(scope, args.this()) else {
        return;
    };
    let _ = set_text_content_in_reaction_scope(scope, runtime_ptr, handle, &value);
    rv.set_undefined();
}

fn hyperlink_element_to_string_callback<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    interface: &'static str,
    local_name: &'static str,
    mut rv: v8::ReturnValue<'_, v8::Value>,
) {
    let Ok((runtime_ptr, handle)) = node_runtime_and_handle_from_args_or_detached(scope, &args)
    else {
        throw_incompatible_method_receiver(scope, interface, "toString");
        return;
    };
    let runtime = unsafe { &*runtime_ptr };
    if !runtime.dom_host().is_html_element_named(handle, local_name) {
        throw_incompatible_method_receiver(scope, interface, "toString");
        return;
    }
    let value = resolve_url_like_attribute(runtime, handle, "href");
    if let Some(value) = v8_string(scope, &value) {
        rv.set(value.into());
    } else {
        rv.set_empty_string();
    }
}

pub(in crate::native_bridge) fn anchor_to_string_callback<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    rv: v8::ReturnValue<'_, v8::Value>,
) {
    hyperlink_element_to_string_callback(scope, args, "HTMLAnchorElement", "a", rv);
}

pub(in crate::native_bridge) fn area_to_string_callback<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    rv: v8::ReturnValue<'_, v8::Value>,
) {
    hyperlink_element_to_string_callback(scope, args, "HTMLAreaElement", "area", rv);
}
