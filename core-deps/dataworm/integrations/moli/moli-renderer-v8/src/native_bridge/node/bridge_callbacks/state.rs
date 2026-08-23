use super::*;

pub(in crate::native_bridge) fn bridge_child_nodes_callback(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments<'_>,
    mut rv: v8::ReturnValue<'_, v8::Value>,
) {
    let Some(handle) = callback_arg_dom_handle(scope, &args, 0) else {
        rv.set_null();
        return;
    };
    let bridge = args.this();
    let Ok(runtime_ptr) = runtime_ptr_from_object(scope, bridge) else {
        rv.set_null();
        return;
    };
    let runtime = unsafe { &*runtime_ptr };
    let Some(child_handles) = runtime
        .dom_host()
        .node(handle)
        .map(|node| node.child_ids(runtime.dom_host().dom()).collect::<Vec<_>>())
    else {
        rv.set_null();
        return;
    };
    let runtime = unsafe { &mut *runtime_ptr };
    let mut values = Vec::with_capacity(child_handles.len());
    for child_handle in child_handles {
        let Some(child) = runtime
            .native_bridge_mut()
            .wrap_handle(scope, runtime_ptr, child_handle)
        else {
            rv.set_null();
            return;
        };
        let child_value: v8::Local<'_, v8::Value> = child.into();
        values.push(v8::Global::new(scope, child_value));
    }
    let list = build_collection_wrapper(scope, runtime_ptr, &values, CollectionKind::NodeList);
    rv.set(list.into());
}

pub(in crate::native_bridge) fn bridge_text_content_callback(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments<'_>,
    mut rv: v8::ReturnValue<'_, v8::Value>,
) {
    let Some(handle) = callback_arg_dom_handle(scope, &args, 0) else {
        rv.set_null();
        return;
    };
    let bridge = args.this();
    let Ok(runtime_ptr) = runtime_ptr_from_object(scope, bridge) else {
        rv.set_null();
        return;
    };
    let runtime = unsafe { &*runtime_ptr };
    let Some(value) = runtime
        .dom_host()
        .node(handle)
        .map(|node| node.text_content(runtime.dom_host().dom()))
    else {
        rv.set_null();
        return;
    };
    let value = v8_string(scope, &value).unwrap_or_else(|| v8::String::empty(scope));
    rv.set(value.into());
}

pub(in crate::native_bridge) fn bridge_describe_node_callback(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments<'_>,
    mut rv: v8::ReturnValue<'_, v8::Value>,
) {
    let Some(handle) = callback_arg_dom_handle(scope, &args, 0) else {
        rv.set_null();
        return;
    };
    let bridge = args.this();
    let Ok(runtime_ptr) = runtime_ptr_from_object(scope, bridge) else {
        rv.set_null();
        return;
    };
    let Some(metadata) = unsafe { &*runtime_ptr }
        .dom_host()
        .node(handle)
        .map(Node::metadata)
    else {
        rv.set_null();
        return;
    };
    let Some(object) = build_node_metadata_object(scope, &metadata) else {
        rv.set_null();
        return;
    };
    rv.set(object.into());
}

pub(in crate::native_bridge) fn bridge_contains_callback(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments<'_>,
    mut rv: v8::ReturnValue<'_, v8::Value>,
) {
    let Some(parent) = callback_arg_dom_handle(scope, &args, 0) else {
        rv.set(v8::Boolean::new(scope, false).into());
        return;
    };
    let Some(child) = callback_arg_dom_handle(scope, &args, 1) else {
        rv.set(v8::Boolean::new(scope, false).into());
        return;
    };
    let bridge = args.this();
    let Ok(runtime_ptr) = runtime_ptr_from_object(scope, bridge) else {
        rv.set(v8::Boolean::new(scope, false).into());
        return;
    };
    rv.set(
        v8::Boolean::new(
            scope,
            unsafe { &*runtime_ptr }
                .dom_host()
                .node(parent)
                .is_some_and(|node| {
                    node.contains(unsafe { &*runtime_ptr }.dom_host().dom(), child)
                }),
        )
        .into(),
    );
}
