use crate::context_bootstrap::bridge_descriptor::BridgeDescriptor;

pub(super) fn install_element_accessors<'s, 'i>(
    _scope: &mut v8::PinScope<'s, 'i, ()>,
    _template: v8::Local<'s, v8::ObjectTemplate>,
    _descriptor: &BridgeDescriptor,
) {
    // GlobalEventHandlers resolve through their owner prototypes; wrappers
    // should not expose event handler attributes as instance own properties.
}
