use super::*;

mod index;
mod object_store;

pub(super) fn install_object_store_and_index_template_bindings<'s>(
    scope: &mut v8::PinScope<'s, '_, ()>,
    prototype: v8::Local<'s, v8::ObjectTemplate>,
    interface_name: &str,
) {
    match interface_name {
        "IDBObjectStore" => object_store::install_object_store_template_bindings(scope, prototype),
        "IDBIndex" => index::install_index_template_bindings(scope, prototype),
        _ => {}
    }
}
