use super::operation::object_store_write_callback;
use crate::webidl;

#[derive(webidl::WebIdlArgs)]
#[webidl(prefix = "IDBObjectStore.put")]
struct IdbObjectStorePutArgs<'s> {
    #[webidl(required, converter = "raw")]
    value: v8::Local<'s, v8::Value>,
    #[webidl(index = 1, converter = "raw")]
    key: Option<v8::Local<'s, v8::Value>>,
}

pub(in crate::context_bootstrap::indexed_db) fn idb_object_store_put_callback<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    rv: v8::ReturnValue<'s, v8::Value>,
) {
    let Some(parsed) = webidl::parse_args::<IdbObjectStorePutArgs<'s>>(scope, &args) else {
        return;
    };
    let key = parsed.key.unwrap_or_else(|| v8::undefined(scope).into());
    object_store_write_callback(scope, args, parsed.value, key, rv, false);
}
