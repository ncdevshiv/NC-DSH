use super::*;
use moli_webapi_declare::WebApiFunctionTemplate;

#[derive(WebApiFunctionTemplate)]
#[webapi(name = "IDBEventTarget", enumerable)]
struct IdbEventTargetPrototypeDeclaration {
    #[webapi(method, length = 2, callback = idb_event_target_add_event_listener_callback)]
    add_event_listener: (),
    #[webapi(
        method,
        length = 2,
        callback = idb_event_target_remove_event_listener_callback
    )]
    remove_event_listener: (),
    #[webapi(method, length = 1, callback = idb_event_target_dispatch_event_callback)]
    dispatch_event: (),
}

pub(super) fn install_idb_event_target_methods<'s>(
    scope: &mut v8::PinScope<'s, '_, ()>,
    prototype: v8::Local<'s, v8::ObjectTemplate>,
) {
    IdbEventTargetPrototypeDeclaration::initialize_prototype_template(scope, prototype);
}
