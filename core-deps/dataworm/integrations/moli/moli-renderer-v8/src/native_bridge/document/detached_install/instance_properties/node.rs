use crate::util::v8str;

pub(in crate::native_bridge::document) fn install_detached_node_core_instance_properties(
    scope: &mut v8::PinScope<'_, '_>,
    object: v8::Local<'_, v8::Object>,
) {
    for name in [
        "nodeType",
        "nodeName",
        "parentNode",
        "parentElement",
        "ownerDocument",
        "childNodes",
        "firstChild",
        "lastChild",
        "previousSibling",
        "nextSibling",
        "isConnected",
        "textContent",
        "nodeValue",
    ] {
        let _ = object.delete(scope, v8str(scope, name).into());
    }
}

pub(in crate::native_bridge::document) fn install_detached_parent_node_instance_properties(
    scope: &mut v8::PinScope<'_, '_>,
    object: v8::Local<'_, v8::Object>,
) {
    for name in [
        "children",
        "firstElementChild",
        "lastElementChild",
        "childElementCount",
    ] {
        let _ = object.delete(scope, v8str(scope, name).into());
    }
}

pub(in crate::native_bridge::document) fn install_detached_non_document_type_child_node_instance_properties(
    scope: &mut v8::PinScope<'_, '_>,
    object: v8::Local<'_, v8::Object>,
) {
    for name in ["previousElementSibling", "nextElementSibling"] {
        let _ = object.delete(scope, v8str(scope, name).into());
    }
}

pub(in crate::native_bridge::document) fn install_detached_document_type_instance_properties(
    scope: &mut v8::PinScope<'_, '_>,
    object: v8::Local<'_, v8::Object>,
) {
    install_detached_node_core_instance_properties(scope, object);
}
