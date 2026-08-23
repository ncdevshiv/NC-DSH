use super::construction_invocation::CustomElementConstructorInvocation;
use super::parser_handoff_failure::reset_parser_failed_custom_element_construction_artifacts;
use super::{
    ConstructionFailure, CustomElementRegistryKey, FailedExistingConstructionPrototype,
    dispatch_form_association_callback_if_needed, dispatch_form_disabled_callback_if_needed,
    fail_existing_custom_element_construction, set_dom_custom_element_state,
    set_wrapper_custom_element_constructor_prototype, validate_custom_element_construction_result,
};
use crate::{
    document_runtime::DomHandle, dom::native::CustomElementState, native_bridge::JsContextHost,
    script_vm::perform_microtask_checkpoint_and_report_pending_promise_rejections,
};

pub(super) struct ParserDirectConstructionContext<'s, 'a> {
    pub(super) constructor: v8::Local<'s, v8::Function>,
    pub(super) definition_name: &'a str,
    pub(super) local_name: &'a str,
    pub(super) registry_key: CustomElementRegistryKey,
    pub(super) original_handle: DomHandle,
    pub(super) initial_owner_document: Option<DomHandle>,
}

pub(super) fn handle_parser_direct_constructor_invocation<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    host_ptr: *mut JsContextHost,
    invocation: CustomElementConstructorInvocation,
    context: ParserDirectConstructionContext<'s, '_>,
) -> DomHandle {
    match invocation {
        CustomElementConstructorInvocation::Created(created) => {
            handle_parser_direct_created_element(scope, host_ptr, created, context)
        }
        CustomElementConstructorInvocation::Exception(exception) => {
            fail_parser_direct_construction(
                scope,
                host_ptr,
                context.original_handle,
                context.constructor,
                ConstructionFailure::Exception(v8::Local::new(scope, &exception)),
            );
            context.original_handle
        }
        CustomElementConstructorInvocation::Empty => {
            fail_parser_direct_construction(
                scope,
                host_ptr,
                context.original_handle,
                context.constructor,
                ConstructionFailure::TypeError(
                    "Custom element constructor did not create an element",
                ),
            );
            context.original_handle
        }
    }
}

fn handle_parser_direct_created_element<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    host_ptr: *mut JsContextHost,
    created: v8::Global<v8::Object>,
    context: ParserDirectConstructionContext<'s, '_>,
) -> DomHandle {
    // Parser-created construction runs a checkpoint before validation so
    // constructor-scheduled microtasks can still invalidate the result before
    // parser attributes or children are transferred.
    perform_microtask_checkpoint_and_report_pending_promise_rejections(scope);
    let created = v8::Local::new(scope, &created);
    match validate_custom_element_construction_result(
        scope,
        host_ptr,
        None,
        created,
        context.definition_name,
        context.local_name,
        context.initial_owner_document,
    ) {
        Ok(constructed_handle) => {
            mark_parser_direct_construction_success(
                scope,
                host_ptr,
                context.original_handle,
                constructed_handle,
                created,
                context.constructor,
                context.registry_key,
                context.definition_name,
            );
            constructed_handle
        }
        Err(failure) => {
            reset_parser_failed_custom_element_construction_artifacts(
                host_ptr,
                context.original_handle,
            );
            fail_existing_custom_element_construction(
                scope,
                host_ptr,
                context.original_handle,
                context.constructor,
                failure,
                FailedExistingConstructionPrototype::ResetToUnknown,
            );
            context.original_handle
        }
    }
}

fn mark_parser_direct_construction_success<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    host_ptr: *mut JsContextHost,
    original_handle: DomHandle,
    constructed_handle: DomHandle,
    created: v8::Local<'s, v8::Object>,
    constructor: v8::Local<'s, v8::Function>,
    registry_key: CustomElementRegistryKey,
    definition_name: &str,
) {
    if constructed_handle != original_handle {
        unsafe { &mut *host_ptr }
            .custom_elements_mut_for_registry_key(registry_key)
            .mark_upgraded_handle(original_handle, definition_name);
        set_dom_custom_element_state(host_ptr, original_handle, CustomElementState::Custom);
        unsafe { &mut *host_ptr }
            .custom_elements_mut_for_registry_key(registry_key)
            .finish_construction(original_handle);
    }
    unsafe { &mut *host_ptr }
        .custom_elements_mut_for_registry_key(registry_key)
        .mark_upgraded_handle(constructed_handle, definition_name);
    set_wrapper_custom_element_constructor_prototype(scope, created, constructor);
    set_dom_custom_element_state(host_ptr, constructed_handle, CustomElementState::Custom);
    unsafe { &mut *host_ptr }
        .custom_elements_mut_for_registry_key(registry_key)
        .finish_construction(constructed_handle);
    dispatch_form_association_callback_if_needed(scope, host_ptr, constructed_handle);
    dispatch_form_disabled_callback_if_needed(scope, host_ptr, constructed_handle);
}

fn fail_parser_direct_construction<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    host_ptr: *mut JsContextHost,
    handle: DomHandle,
    constructor: v8::Local<'s, v8::Function>,
    failure: ConstructionFailure<'s>,
) {
    reset_parser_failed_custom_element_construction_artifacts(host_ptr, handle);
    fail_existing_custom_element_construction(
        scope,
        host_ptr,
        handle,
        constructor,
        failure,
        FailedExistingConstructionPrototype::ResetToUnknown,
    );
}
