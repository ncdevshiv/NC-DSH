use crate::context_bootstrap::bridge_descriptor::BridgeDescriptor;

pub(super) fn install_document_accessors<'s, 'i>(
    _scope: &mut v8::PinScope<'s, 'i, ()>,
    _template: v8::Local<'s, v8::ObjectTemplate>,
    _descriptor: &BridgeDescriptor,
) {
}
