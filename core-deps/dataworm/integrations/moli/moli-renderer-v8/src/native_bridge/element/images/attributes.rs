use super::super::super::node::node_runtime_and_handle_from_object_or_detached;
use super::super::{
    boolean_attribute_property_getter_from_object_or_detached, set_reflected_boolean_attribute,
};

pub(in crate::native_bridge) fn image_is_map_getter_function<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    rv: v8::ReturnValue<'s, v8::Value>,
) {
    boolean_attribute_property_getter_from_object_or_detached(scope, args.this(), "ismap", rv);
}

pub(in crate::native_bridge) fn image_is_map_setter_function<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue<'s, v8::Value>,
) {
    let Ok((runtime_ptr, handle)) =
        node_runtime_and_handle_from_object_or_detached(scope, args.this())
    else {
        rv.set_undefined();
        return;
    };
    set_reflected_boolean_attribute(
        scope,
        runtime_ptr,
        handle,
        "ismap",
        args.get(0).boolean_value(scope),
    );
    rv.set_undefined();
}
