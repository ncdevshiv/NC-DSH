use crate::native_bridge::collections as bridge_collections;
use moli_webapi_declare::WebApiFunctionTemplate;

#[derive(WebApiFunctionTemplate)]
#[webapi(name = "NativeBridgeCollectionQueries", enumerable)]
struct NativeBridgeCollectionQueriesDeclaration {
    #[webapi(
        method = "getElementsByTagName",
        callback = bridge_collections::bridge_get_elements_by_tag_name_callback
    )]
    get_elements_by_tag_name: (),

    #[webapi(
        method = "getElementsByTagNameNS",
        callback = bridge_collections::bridge_get_elements_by_tag_name_ns_callback
    )]
    get_elements_by_tag_name_ns: (),

    #[webapi(
        method = "getElementsByClassName",
        callback = bridge_collections::bridge_get_elements_by_class_name_callback
    )]
    get_elements_by_class_name: (),

    #[webapi(
        method = "getElementsByName",
        callback = bridge_collections::bridge_get_elements_by_name_callback
    )]
    get_elements_by_name: (),

    #[webapi(
        method = "resolveLiveCollection",
        callback = bridge_collections::bridge_resolve_live_collection_callback
    )]
    resolve_live_collection: (),
}

#[derive(WebApiFunctionTemplate)]
#[webapi(name = "NativeBridgeCollectionBuilders", enumerable)]
struct NativeBridgeCollectionBuildersDeclaration {
    #[webapi(
        method = "createNodeList",
        callback = bridge_collections::bridge_create_node_list_callback
    )]
    create_node_list: (),

    #[webapi(
        method = "createHtmlCollection",
        callback = bridge_collections::bridge_create_html_collection_callback
    )]
    create_html_collection: (),

    #[webapi(
        method = "createLiveNodeList",
        callback = bridge_collections::bridge_create_live_node_list_callback
    )]
    create_live_node_list: (),

    #[webapi(
        method = "createLiveHtmlCollection",
        callback = bridge_collections::bridge_create_live_html_collection_callback
    )]
    create_live_html_collection: (),
}

pub(super) fn install_collection_queries<'s, 'i>(
    scope: &mut v8::PinScope<'s, 'i, ()>,
    template: v8::Local<'s, v8::ObjectTemplate>,
) {
    NativeBridgeCollectionQueriesDeclaration::initialize_prototype_template(scope, template);
}

pub(super) fn install_collection_builders<'s, 'i>(
    scope: &mut v8::PinScope<'s, 'i, ()>,
    template: v8::Local<'s, v8::ObjectTemplate>,
) {
    NativeBridgeCollectionBuildersDeclaration::initialize_prototype_template(scope, template);
}
