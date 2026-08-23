use v8::{Local, NewStringType, PinScope, String, Value};

// Prefer this for fixed property/slot names. With `&'static str` we can create
// an internalized V8 string directly, avoid the fallible dynamic-string path,
// and reuse V8's canonical key for repeated property definitions/lookups.
pub fn v8str<'a>(scope: &PinScope<'a, '_, ()>, value: &'static str) -> Local<'a, String> {
    String::new_from_utf8(scope, value.as_bytes(), NewStringType::Internalized)
        .expect("v8 internalized string creation failed")
}

pub fn v8_string<'a>(scope: &PinScope<'a, '_, ()>, value: &str) -> Option<Local<'a, String>> {
    String::new(scope, value)
}

pub fn is_nullish(value: Local<'_, Value>) -> bool {
    value.is_null_or_undefined()
}
