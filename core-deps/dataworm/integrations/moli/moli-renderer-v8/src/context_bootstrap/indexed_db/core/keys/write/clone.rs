use super::*;

pub(super) fn clone_js_value<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    value: v8::Local<'s, v8::Value>,
) -> Option<v8::Local<'s, v8::Value>> {
    let bytes = serialize_js_value(scope, value)?;
    deserialize_js_value(scope, &bytes)
}
