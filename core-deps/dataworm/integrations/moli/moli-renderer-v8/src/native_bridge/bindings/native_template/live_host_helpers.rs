use crate::native_bridge::{element, node};
use moli_webapi_declare::WebApiFunctionTemplate;

#[derive(WebApiFunctionTemplate)]
#[webapi(name = "NativeBridgeLiveNodeAndElementHelpers", enumerable)]
struct NativeBridgeLiveNodeAndElementHelpersDeclaration {
    #[webapi(method, callback = node::bridge_owner_document_callback)]
    owner_document: (),

    #[webapi(method, callback = node::bridge_parent_node_callback)]
    parent_node: (),

    #[webapi(method, callback = node::bridge_first_child_callback)]
    first_child: (),

    #[webapi(method, callback = node::bridge_last_child_callback)]
    last_child: (),

    #[webapi(method, callback = node::bridge_next_sibling_callback)]
    next_sibling: (),

    #[webapi(method, callback = node::bridge_previous_sibling_callback)]
    previous_sibling: (),

    #[webapi(method, callback = node::bridge_child_nodes_callback)]
    child_nodes: (),

    #[webapi(method, callback = node::bridge_text_content_callback)]
    text_content: (),

    #[webapi(method, callback = node::bridge_describe_node_callback)]
    describe_node: (),

    #[webapi(method, callback = node::bridge_append_child_callback)]
    append_child: (),

    #[webapi(method, callback = node::bridge_remove_child_callback)]
    remove_child: (),

    #[webapi(method, callback = node::bridge_insert_before_callback)]
    insert_before: (),

    #[webapi(method, callback = node::bridge_contains_callback)]
    contains: (),

    #[webapi(method, callback = node::bridge_set_text_content_callback)]
    set_text_content: (),

    #[webapi(method, callback = element::bridge_get_attribute_callback)]
    get_attribute: (),

    #[webapi(method, callback = element::bridge_set_attribute_callback)]
    set_attribute: (),

    #[webapi(method, callback = element::bridge_remove_attribute_callback)]
    remove_attribute: (),

    #[webapi(method, callback = element::bridge_set_input_value_callback)]
    set_input_value: (),

    #[webapi(method, callback = element::bridge_set_checked_state_callback)]
    set_checked_state: (),

    #[webapi(method, callback = element::bridge_set_selected_state_callback)]
    set_selected_state: (),

    #[webapi(method, callback = element::bridge_set_indeterminate_state_callback)]
    set_indeterminate_state: (),
}

pub(super) fn install_live_node_and_element_helpers<'s, 'i>(
    scope: &mut v8::PinScope<'s, 'i, ()>,
    template: v8::Local<'s, v8::ObjectTemplate>,
) {
    NativeBridgeLiveNodeAndElementHelpersDeclaration::initialize_prototype_template(
        scope, template,
    );
}
