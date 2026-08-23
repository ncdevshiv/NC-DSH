use super::*;
use crate::util::set_private_value;

pub(crate) fn xhr_constructor_callback(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments<'_>,
    mut rv: v8::ReturnValue<'_, v8::Value>,
) {
    if !args.is_construct_call() {
        throw_type_error(
            scope,
            "Failed to construct 'XMLHttpRequest': Please use the 'new' operator.",
        );
        return;
    }

    let xhr = args.this();
    let execution_context = context_host_ptr_from_global_bridge(scope).and_then(|host_ptr| {
        unsafe { &*host_ptr }.current_runtime_window_execution_context_binding(scope)
    });
    if let Some(crate::native_bridge::OwnerDispatchScope::Child(handle)) = execution_context
        .as_ref()
        .map(crate::native_bridge::WindowExecutionContextBinding::dispatch_scope)
    {
        let handle = v8::Number::new(scope, handle.index() as f64);
        set_private_value(scope, xhr, XHR_CHILD_CONTEXT_HANDLE_SLOT, handle.into());
    }
    initialize_xml_http_request_instance(scope, xhr, execution_context.as_ref());

    rv.set(xhr.into());
}
