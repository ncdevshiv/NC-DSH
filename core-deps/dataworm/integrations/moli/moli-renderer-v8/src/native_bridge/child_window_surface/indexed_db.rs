use crate::{document_runtime::DomHandle, util::context_host_ptr_from_global_bridge};

pub(in crate::native_bridge) fn bind_materialized_child_window_indexed_db_factory<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    window: v8::Local<'s, v8::Object>,
    handle: DomHandle,
) {
    let Some(host_ptr) = context_host_ptr_from_global_bridge(scope) else {
        return;
    };
    let Some(factory) =
        crate::context_bootstrap::materialized_indexed_db_factory_for_window(scope, window)
    else {
        return;
    };
    let dispatch_scope = crate::native_bridge::OwnerDispatchScope::Child(handle);
    if let Some(execution_context) =
        unsafe { &*host_ptr }.current_registered_window_execution_context_identity(dispatch_scope)
    {
        let _ = crate::context_bootstrap::bind_indexed_db_factory_to_window_execution_context(
            scope,
            factory,
            execution_context,
        );
    }
}
