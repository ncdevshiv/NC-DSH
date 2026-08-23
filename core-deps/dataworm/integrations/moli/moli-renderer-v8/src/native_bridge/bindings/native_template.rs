mod collections;
mod detached_bootstrap;
mod detached_document_state;
mod detached_mutation_attributes;
mod detached_node_surface;
mod document_factory;
mod live_host_helpers;
mod roots;

pub(super) fn build_native_bridge_template<'s, 'i>(
    scope: &mut v8::PinScope<'s, 'i, ()>,
) -> v8::Local<'s, v8::ObjectTemplate> {
    let template = v8::ObjectTemplate::new(scope);
    let _ = template.set_internal_field_count(2);

    roots::install_roots_and_document_lookup(scope, template);
    collections::install_collection_queries(scope, template);
    document_factory::install_live_document_factory(scope, template);
    collections::install_collection_builders(scope, template);
    detached_bootstrap::install_detached_bootstrap(scope, template);
    detached_bootstrap::install_detached_creation_helpers(scope, template);
    detached_document_state::install_detached_document_state(scope, template);
    detached_node_surface::install_detached_node_surface(scope, template);
    detached_mutation_attributes::install_detached_mutation_and_attribute_helpers(scope, template);
    live_host_helpers::install_live_node_and_element_helpers(scope, template);

    template
}
