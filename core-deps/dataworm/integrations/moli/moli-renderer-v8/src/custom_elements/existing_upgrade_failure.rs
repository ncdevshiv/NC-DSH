use super::construction_failure::{
    ConstructionFailure, report_custom_element_construction_failure,
};
use super::construction_result::{
    FailedExistingConstructionPrototype, set_wrapper_custom_element_constructor_prototype,
};
use super::element_state::set_dom_custom_element_state;
use crate::dom::native::CustomElementState;

use super::super::{
    document_runtime::DomHandle,
    dom_parser::DOM_PARSER_FOREIGN_NODE_SLOT,
    native_bridge::JsContextHost,
    util::{get_private_object, global_constructor_prototype},
};

pub(super) fn fail_existing_custom_element_construction<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    host_ptr: *mut JsContextHost,
    handle: DomHandle,
    constructor: v8::Local<'s, v8::Function>,
    failure: ConstructionFailure<'s>,
    failure_prototype: FailedExistingConstructionPrototype,
) {
    let wrapper = unsafe { &mut *host_ptr }
        .native_bridge_mut()
        .wrap_handle(scope, host_ptr, handle);
    let consumed_pending_wrapper = unsafe { &*host_ptr }
        .custom_elements_for_node_handle(handle)
        .is_some_and(|store| store.pending_construction_is_already_constructed(handle));
    unsafe { &mut *host_ptr }
        .custom_elements_mut_for_node_handle(handle)
        .discard_pending_construction(handle);
    unsafe { &mut *host_ptr }
        .custom_elements_mut_for_node_handle(handle)
        .mark_failed_construction_handle(handle);
    unsafe { &mut *host_ptr }
        .custom_element_reactions_mut()
        .clear_reactions(handle);
    set_dom_custom_element_state(host_ptr, handle, CustomElementState::Failed);
    if let Some(wrapper) = wrapper {
        match failure_prototype {
            FailedExistingConstructionPrototype::ResetToUnknown => {
                set_wrapper_failed_custom_element_prototype(scope, wrapper);
            }
            FailedExistingConstructionPrototype::PreserveCurrent if consumed_pending_wrapper => {
                set_wrapper_custom_element_constructor_prototype(scope, wrapper, constructor);
            }
            FailedExistingConstructionPrototype::PreserveCurrent => {}
        }
    }
    report_custom_element_construction_failure(scope, host_ptr, Some(constructor), failure);
}

fn set_wrapper_failed_custom_element_prototype<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    wrapper: v8::Local<'s, v8::Object>,
) {
    let Some(prototype) = global_constructor_prototype(scope, "HTMLUnknownElement") else {
        return;
    };
    let prototype = prototype.into();
    let _ = wrapper.set_prototype(scope, prototype);
    if let Some(foreign) = get_private_object(scope, wrapper, DOM_PARSER_FOREIGN_NODE_SLOT) {
        let _ = foreign.set_prototype(scope, prototype);
    }
}
