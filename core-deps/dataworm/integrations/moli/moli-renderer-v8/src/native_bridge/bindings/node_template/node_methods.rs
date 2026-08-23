use crate::context_bootstrap::bridge_descriptor::BridgeDescriptor;

pub(super) fn install_tree_mutation_and_relationship_methods<'s, 'i>(
    _scope: &mut v8::PinScope<'s, 'i, ()>,
    _template: v8::Local<'s, v8::ObjectTemplate>,
    _descriptor: &BridgeDescriptor,
) {
    // Standard Node and DOM mixin methods are installed on their WebIDL
    // prototype owners by native_bridge::node::install_node_prototype_reflection_surface.
}

pub(super) fn install_node_utility_methods<'s, 'i>(
    _scope: &mut v8::PinScope<'s, 'i, ()>,
    _template: v8::Local<'s, v8::ObjectTemplate>,
) {
    // Standard Node utility methods are installed on Node.prototype.
}
