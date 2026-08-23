use super::*;

pub(in crate::native_bridge::document) fn install_detached_document_instance_properties(
    scope: &mut v8::PinScope<'_, '_>,
    object: v8::Local<'_, v8::Object>,
    _kind: &str,
) {
    install_detached_node_core_instance_properties(scope, object);
    install_detached_parent_node_instance_properties(scope, object);
}
