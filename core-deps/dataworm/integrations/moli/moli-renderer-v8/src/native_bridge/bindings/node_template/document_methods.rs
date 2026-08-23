use crate::context_bootstrap::bridge_descriptor::BridgeDescriptor;

pub(super) fn install_document_factory_methods<'s, 'i>(
    _scope: &mut v8::PinScope<'s, 'i, ()>,
    _template: v8::Local<'s, v8::ObjectTemplate>,
    _descriptor: &BridgeDescriptor,
) {
    // Live document methods resolve through Document.prototype so wrapped documents
    // match Chromium's instance shape and do not expose these methods as own props.
}

pub(super) fn install_document_lifecycle_and_query_methods<'s, 'i>(
    _scope: &mut v8::PinScope<'s, 'i, ()>,
    _template: v8::Local<'s, v8::ObjectTemplate>,
    descriptor: &BridgeDescriptor,
) {
    let _ = descriptor.install_groups.document_methods
        || descriptor.install_groups.markup_container_api;
    // Live document and markup-container query methods resolve through their
    // owner prototypes so wrappers do not expose them as own properties.
}
