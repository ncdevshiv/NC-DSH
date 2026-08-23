use super::super::*;

pub(in crate::native_bridge) fn input_checked_getter_function<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue<'_, v8::Value>,
) {
    input_checked_getter_from_object(scope, args.this(), &mut rv);
}

fn input_checked_getter_from_object<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    object: v8::Local<'s, v8::Object>,
    rv: &mut v8::ReturnValue<'_, v8::Value>,
) {
    let Ok((runtime_ptr, handle)) = node_runtime_and_handle_from_object_or_detached(scope, object)
    else {
        rv.set_bool(false);
        return;
    };
    let runtime = unsafe { &*runtime_ptr };
    rv.set_bool(
        runtime
            .dom_host()
            .node(handle)
            .and_then(Node::as_element)
            .is_some_and(Element::checked),
    );
}

pub(in crate::native_bridge) fn input_checked_setter_function<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue<'_, v8::Value>,
) {
    let Ok((runtime_ptr, handle)) =
        node_runtime_and_handle_from_object_or_detached(scope, args.this())
    else {
        rv.set_undefined();
        return;
    };
    let checked = args.get(0).boolean_value(scope);
    let runtime = unsafe { &mut *runtime_ptr };
    let _ = runtime.set_checked_state(scope, runtime_ptr, handle, checked);
    rv.set_undefined();
}

pub(in crate::native_bridge) fn input_indeterminate_getter_function<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue<'_, v8::Value>,
) {
    input_indeterminate_getter_from_object(scope, args.this(), &mut rv);
}

fn input_indeterminate_getter_from_object<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    object: v8::Local<'s, v8::Object>,
    rv: &mut v8::ReturnValue<'_, v8::Value>,
) {
    let Ok((runtime_ptr, handle)) = node_runtime_and_handle_from_object_or_detached(scope, object)
    else {
        rv.set_bool(false);
        return;
    };
    let runtime = unsafe { &*runtime_ptr };
    rv.set_bool(
        runtime
            .dom_host()
            .node(handle)
            .and_then(Node::as_element)
            .is_some_and(Element::indeterminate),
    );
}

pub(in crate::native_bridge) fn input_indeterminate_setter_function<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue<'_, v8::Value>,
) {
    let Ok((runtime_ptr, handle)) =
        node_runtime_and_handle_from_object_or_detached(scope, args.this())
    else {
        rv.set_undefined();
        return;
    };
    let _ = unsafe { &mut *runtime_ptr }.set_indeterminate_state(
        scope,
        runtime_ptr,
        handle,
        args.get(0).boolean_value(scope),
    );
    rv.set_undefined();
}
