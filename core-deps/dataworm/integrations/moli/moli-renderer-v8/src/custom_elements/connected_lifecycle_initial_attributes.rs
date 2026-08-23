use super::super::{document_runtime::DomHandle, native_bridge::JsContextHost};
use super::PendingInitialAttribute;
use super::attribute_lifecycle::enqueue_attribute_changed_callback;

pub(super) fn enqueue_pending_initial_attribute_callbacks(
    scope: &mut v8::PinScope<'_, '_>,
    host_ptr: *mut JsContextHost,
    handle: DomHandle,
) -> bool {
    let pending_attributes = unsafe { &mut *host_ptr }
        .custom_elements_mut_for_node_handle(handle)
        .take_pending_initial_attributes(handle);
    let mut enqueued = false;
    for PendingInitialAttribute {
        name,
        namespace,
        value,
    } in pending_attributes
    {
        if enqueue_attribute_changed_callback(
            scope,
            host_ptr,
            handle,
            &name,
            namespace.as_deref(),
            None,
            Some(&value),
        ) {
            enqueued = true;
        }
    }
    enqueued
}
