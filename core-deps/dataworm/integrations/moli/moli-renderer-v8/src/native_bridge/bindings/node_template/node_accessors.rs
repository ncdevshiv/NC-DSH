use crate::context_bootstrap::bridge_descriptor::BridgeDescriptor;

pub(super) fn install_node_accessors<'s, 'i>(
    scope: &mut v8::PinScope<'s, 'i, ()>,
    template: v8::Local<'s, v8::ObjectTemplate>,
    descriptor: &BridgeDescriptor,
) {
    super::character_data::install_character_data_api(scope, template, descriptor);
}
