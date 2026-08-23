use crate::native_bridge::document;
use moli_webapi_declare::WebApiFunctionTemplate;

#[derive(WebApiFunctionTemplate)]
#[webapi(name = "NativeBridgeLiveDocumentFactory", enumerable)]
struct NativeBridgeLiveDocumentFactoryDeclaration {
    #[webapi(method, callback = document::bridge_create_element_callback)]
    create_element: (),

    #[webapi(
        method = "createElementNS",
        callback = document::bridge_create_element_ns_callback
    )]
    create_element_ns: (),

    #[webapi(method, callback = document::bridge_create_text_node_callback)]
    create_text_node: (),

    #[webapi(method, callback = document::bridge_create_comment_callback)]
    create_comment: (),

    #[webapi(
        method,
        callback = document::bridge_create_processing_instruction_callback
    )]
    create_processing_instruction: (),

    #[webapi(
        method = "createCDATASection",
        callback = document::bridge_create_cdata_section_not_supported_callback
    )]
    create_cdata_section: (),
}

pub(super) fn install_live_document_factory<'s, 'i>(
    scope: &mut v8::PinScope<'s, 'i, ()>,
    template: v8::Local<'s, v8::ObjectTemplate>,
) {
    NativeBridgeLiveDocumentFactoryDeclaration::initialize_prototype_template(scope, template);
}
