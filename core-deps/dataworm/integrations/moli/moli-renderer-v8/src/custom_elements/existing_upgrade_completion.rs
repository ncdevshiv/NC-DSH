use super::construction_result::set_wrapper_custom_element_constructor_prototype;
use super::element_state::set_dom_custom_element_state;
use super::{PendingInitialAttribute, dispatch_attribute_changed_callback};
use crate::{
    document_runtime::DomHandle,
    dom::native::{CustomElementState, Node},
    native_bridge::JsContextHost,
};

pub(super) fn complete_existing_custom_element_upgrade<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    host_ptr: *mut JsContextHost,
    handle: DomHandle,
    wrapper: v8::Local<'s, v8::Object>,
    constructor: v8::Local<'s, v8::Function>,
    definition_name: &str,
    initial_attributes: Vec<PendingInitialAttribute>,
) {
    unsafe { &mut *host_ptr }
        .custom_elements_mut_for_node_handle(handle)
        .mark_upgraded_handle(handle, definition_name);
    set_wrapper_custom_element_constructor_prototype(scope, wrapper, constructor);
    set_dom_custom_element_state(host_ptr, handle, CustomElementState::Custom);
    unsafe { &mut *host_ptr }
        .custom_elements_mut_for_node_handle(handle)
        .finish_construction(handle);
    if !initial_attributes.is_empty() {
        unsafe { &mut *host_ptr }
            .custom_elements_mut_for_node_handle(handle)
            .mark_pending_initial_attributes(handle, initial_attributes);
        deliver_pending_initial_attribute_callbacks(scope, host_ptr, handle);
    }
}

pub(super) fn observed_attributes_with_current_values(
    host_ptr: *mut JsContextHost,
    handle: DomHandle,
    definition_name: &str,
) -> Vec<PendingInitialAttribute> {
    let Some(observed_attributes) = unsafe { &*host_ptr }
        .custom_elements_for_node_handle(handle)
        .and_then(|store| store.observed_attributes_for_definition(definition_name))
    else {
        return Vec::new();
    };
    let Some(element) = unsafe { &*host_ptr }
        .dom_host()
        .node(handle)
        .and_then(Node::as_element)
    else {
        return Vec::new();
    };
    element
        .attributes()
        .iter()
        .filter(|attribute| {
            observed_attributes
                .iter()
                .any(|observed| observed == attribute.local_name())
        })
        .map(|attribute| PendingInitialAttribute {
            name: attribute.local_name().to_owned(),
            namespace: (!attribute.namespace().is_empty())
                .then(|| attribute.namespace().to_owned()),
            value: attribute.value().to_owned(),
        })
        .collect()
}

fn deliver_pending_initial_attribute_callbacks(
    scope: &mut v8::PinScope<'_, '_>,
    host_ptr: *mut JsContextHost,
    handle: DomHandle,
) {
    let pending_attributes = unsafe { &mut *host_ptr }
        .custom_elements_mut_for_node_handle(handle)
        .take_pending_initial_attributes(handle);
    for PendingInitialAttribute {
        name,
        namespace,
        value,
    } in pending_attributes
    {
        dispatch_attribute_changed_callback(
            scope,
            host_ptr,
            handle,
            &name,
            namespace.as_deref(),
            None,
            Some(&value),
        );
    }
}
