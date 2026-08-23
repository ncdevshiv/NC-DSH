use crate::context_bootstrap::bridge_descriptor::BridgeDescriptor;

pub(super) fn install_html_element_action_methods<'s, 'i>(
    _scope: &mut v8::PinScope<'s, 'i, ()>,
    _template: v8::Local<'s, v8::ObjectTemplate>,
    _descriptor: &BridgeDescriptor,
) {
}

pub(super) fn install_element_query_and_attribute_methods<'s, 'i>(
    _scope: &mut v8::PinScope<'s, 'i, ()>,
    _template: v8::Local<'s, v8::ObjectTemplate>,
) {
}

pub(super) fn install_extended_element_methods<'s, 'i>(
    _scope: &mut v8::PinScope<'s, 'i, ()>,
    _template: v8::Local<'s, v8::ObjectTemplate>,
    _descriptor: &BridgeDescriptor,
) {
}
