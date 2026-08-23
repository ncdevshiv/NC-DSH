use crate::native_bridge::{document, window};
use moli_webapi_declare::WebApiFunctionTemplate;

#[derive(WebApiFunctionTemplate)]
#[webapi(name = "NativeBridgeRoots", enumerable)]
struct NativeBridgeRootsDeclaration {
    // These are internal bridge slots: callbacks intentionally read the
    // ObjectTemplate holder that owns the native bridge, not an arbitrary
    // JavaScript receiver.
    #[webapi(native_data_property, getter = window::bridge_window_getter)]
    window: (),

    #[webapi(native_data_property, getter = document::bridge_document_getter)]
    document: (),

    #[webapi(method, callback = document::bridge_get_element_by_id_callback)]
    get_element_by_id: (),

    #[webapi(
        method = "__detachedGetElementsByTagName",
        callback = document::bridge_detached_get_elements_by_tag_name_callback
    )]
    detached_get_elements_by_tag_name: (),

    #[webapi(
        method = "__detachedGetElementsByTagNameNS",
        callback = document::bridge_detached_get_elements_by_tag_name_ns_callback
    )]
    detached_get_elements_by_tag_name_ns: (),

    #[webapi(
        method = "__detachedGetElementsByClassName",
        callback = document::bridge_detached_get_elements_by_class_name_callback
    )]
    detached_get_elements_by_class_name: (),

    #[webapi(
        method = "__detachedGetElementsByName",
        callback = document::bridge_detached_get_elements_by_name_callback
    )]
    detached_get_elements_by_name: (),
}

pub(super) fn install_roots_and_document_lookup<'s, 'i>(
    scope: &mut v8::PinScope<'s, 'i, ()>,
    template: v8::Local<'s, v8::ObjectTemplate>,
) {
    NativeBridgeRootsDeclaration::initialize_prototype_template(scope, template);
}
