use crate::native_bridge::document;
use moli_webapi_declare::WebApiFunctionTemplate;

#[derive(WebApiFunctionTemplate)]
#[webapi(name = "NativeBridgeDetachedDocumentState", enumerable)]
struct NativeBridgeDetachedDocumentStateDeclaration {
    #[webapi(
        method = "__detachedGetElementById",
        callback = document::bridge_detached_get_element_by_id_callback
    )]
    detached_get_element_by_id: (),

    #[webapi(
        method = "__detachedQuerySelector",
        callback = document::bridge_detached_query_selector_callback
    )]
    detached_query_selector: (),

    #[webapi(
        method = "__detachedQuerySelectorAll",
        callback = document::bridge_detached_query_selector_all_callback
    )]
    detached_query_selector_all: (),

    #[webapi(
        method = "__detachedMatches",
        callback = document::bridge_detached_matches_callback
    )]
    detached_matches: (),

    #[webapi(
        method = "__detachedDocumentEvaluate",
        callback = document::bridge_detached_document_evaluate_callback
    )]
    detached_document_evaluate: (),

    #[webapi(
        method = "__detachedDocumentElement",
        callback = document::bridge_detached_document_element_callback
    )]
    detached_document_element: (),

    #[webapi(
        method = "__detachedDoctype",
        callback = document::bridge_detached_document_doctype_callback
    )]
    detached_doctype: (),

    #[webapi(
        method = "__detachedHead",
        callback = document::bridge_detached_document_head_callback
    )]
    detached_head: (),

    #[webapi(
        method = "__detachedBody",
        callback = document::bridge_detached_document_body_callback
    )]
    detached_body: (),

    #[webapi(
        method = "__setDetachedDocumentBody",
        callback = document::bridge_detached_document_body_setter_callback
    )]
    set_detached_document_body: (),

    #[webapi(
        method = "__detachedDocumentTitle",
        callback = document::bridge_detached_document_title_getter_callback
    )]
    detached_document_title: (),

    #[webapi(
        method = "__setDetachedDocumentTitle",
        callback = document::bridge_detached_document_title_setter_callback
    )]
    set_detached_document_title: (),

    #[webapi(
        method = "__detachedDocumentReadyState",
        callback = document::bridge_detached_document_ready_state_callback
    )]
    detached_document_ready_state: (),

    #[webapi(
        method = "__detachedDocumentUrl",
        callback = document::bridge_detached_document_url_callback
    )]
    detached_document_url: (),

    #[webapi(
        method = "__detachedDocumentUri",
        callback = document::bridge_detached_document_uri_callback
    )]
    detached_document_uri: (),

    #[webapi(
        method = "__detachedDocumentBaseUri",
        callback = document::bridge_detached_document_base_uri_callback
    )]
    detached_document_base_uri: (),

    #[webapi(
        method = "__detachedDocumentContentType",
        callback = document::bridge_detached_document_content_type_callback
    )]
    detached_document_content_type: (),

    #[webapi(
        method = "__detachedDocumentCharacterSet",
        callback = document::bridge_detached_document_character_set_callback
    )]
    detached_document_character_set: (),

    #[webapi(
        method = "__detachedDocumentCompatMode",
        callback = document::bridge_detached_document_compat_mode_callback
    )]
    detached_document_compat_mode: (),

    #[webapi(
        method = "__detachedDocumentReferrer",
        callback = document::bridge_detached_document_referrer_callback
    )]
    detached_document_referrer: (),

    #[webapi(
        method = "__detachedDocumentDomain",
        callback = document::bridge_detached_document_domain_callback
    )]
    detached_document_domain: (),

    #[webapi(
        method = "__setDetachedDocumentDomain",
        callback = document::bridge_set_detached_document_domain_callback
    )]
    set_detached_document_domain: (),
}

pub(super) fn install_detached_document_state<'s, 'i>(
    scope: &mut v8::PinScope<'s, 'i, ()>,
    template: v8::Local<'s, v8::ObjectTemplate>,
) {
    NativeBridgeDetachedDocumentStateDeclaration::initialize_prototype_template(scope, template);
}
