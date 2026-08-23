use crate::context_bootstrap::bridge_descriptor::BridgeDescriptor;

pub(super) fn install_character_data_api<'s, 'i>(
    _scope: &mut v8::PinScope<'s, 'i, ()>,
    _template: v8::Local<'s, v8::ObjectTemplate>,
    descriptor: &BridgeDescriptor,
) {
    if !descriptor.install_groups.character_data_api {}
}
