use super::super::{document_runtime::DomHandle, native_bridge::JsContextHost};
use super::{definition_name_for_handle, has_pending_upgrade_reaction};

pub(super) fn can_upgrade_handle(host_ptr: *mut JsContextHost, handle: DomHandle) -> bool {
    if has_pending_upgrade_reaction(host_ptr, handle) {
        return false;
    }
    let host = unsafe { &*host_ptr };
    if host
        .custom_elements_for_node_handle(handle)
        .is_some_and(|store| {
            store.is_upgraded_handle(handle) || store.is_pending_construction_handle(handle)
        })
    {
        return false;
    }
    let Some(definition_name) = definition_name_for_handle(host_ptr, handle) else {
        return false;
    };
    host.custom_elements_for_node_handle(handle)
        .is_some_and(|store| store.has_definition(&definition_name))
}
