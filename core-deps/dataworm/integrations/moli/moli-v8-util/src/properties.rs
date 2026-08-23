use v8::{Local, Object, PinScope, PropertyAttribute, Value};

use crate::strings::{v8_string, v8str};

/// Creates a non-web-visible internal dictionary object.
///
/// Use this for renderer-owned state/maps whose string keys are internal or
/// page-derived and may later be read through generic object lookup. Removing
/// `Object.prototype` prevents page prototype pollution from manufacturing
/// internal state.
///
/// Do not use this for Web API instances, option/result objects, or other
/// values whose prototype chain is part of script-visible browser semantics.
pub fn new_null_prototype_object<'s>(scope: &mut PinScope<'s, '_>) -> Local<'s, Object> {
    let object = Object::new(scope);
    set_null_prototype(scope, object);
    object
}

/// Retrofitting helper for internal objects created by V8/serde builders.
///
/// Prefer `new_null_prototype_object` when the object is created locally. This
/// should still be limited to non-web-visible dictionaries and owner state.
pub fn set_null_prototype(scope: &mut PinScope<'_, '_>, object: Local<'_, Object>) {
    let null = v8::null(scope);
    let _ = object.set_prototype(scope, null.into());
}

pub fn get_property<'s>(
    scope: &mut PinScope<'s, '_>,
    object: Local<'_, Object>,
    key: &str,
) -> Option<Local<'s, Value>> {
    object.get(scope, v8_string(scope, key)?.into())
}

pub fn get_static_property<'s>(
    scope: &mut PinScope<'s, '_>,
    object: Local<'_, Object>,
    key: &'static str,
) -> Option<Local<'s, Value>> {
    object.get(scope, v8str(scope, key).into())
}

/// Reads an own real property with a static key.
///
/// V8's `get_real_named_property` skips interceptors but still searches the
/// prototype chain. The explicit `has_real_named_property` guard keeps internal
/// slot lookups own-only without allocating a dynamic V8 string.
pub fn get_own_static_property<'s>(
    scope: &mut PinScope<'s, '_>,
    object: Local<'_, Object>,
    key: &'static str,
) -> Option<Local<'s, Value>> {
    let key = v8str(scope, key);
    if !object
        .has_real_named_property(scope, key.into())
        .unwrap_or(false)
    {
        return None;
    }
    object.get_real_named_property(scope, key.into())
}

/// Reads an own real property with a runtime key.
///
/// Prefer `get_own_static_property` for internal slots and generated binding
/// names that are known at compile time.
pub fn get_own_property<'s>(
    scope: &mut PinScope<'s, '_>,
    object: Local<'_, Object>,
    key: &str,
) -> Option<Local<'s, Value>> {
    let key = v8_string(scope, key)?;
    if !object
        .has_real_named_property(scope, key.into())
        .unwrap_or(false)
    {
        return None;
    }
    object.get_real_named_property(scope, key.into())
}

pub fn set_property(
    scope: &mut PinScope<'_, '_>,
    object: Local<'_, Object>,
    key: &str,
    value: Local<'_, Value>,
) {
    let Some(key) = v8_string(scope, key) else {
        return;
    };
    let _ = object.set(scope, key.into(), value);
}

pub fn set_static_property(
    scope: &mut PinScope<'_, '_>,
    object: Local<'_, Object>,
    key: &'static str,
    value: Local<'_, Value>,
) {
    let _ = object.set(scope, v8str(scope, key).into(), value);
}

pub fn set_string_property(
    scope: &mut PinScope<'_, '_>,
    object: Local<'_, Object>,
    key: &str,
    value: &str,
) {
    let Some(value) = v8_string(scope, value) else {
        return;
    };
    set_property(scope, object, key, value.into());
}

pub fn set_static_string_property(
    scope: &mut PinScope<'_, '_>,
    object: Local<'_, Object>,
    key: &'static str,
    value: &str,
) {
    let Some(value) = v8_string(scope, value) else {
        return;
    };
    set_static_property(scope, object, key, value.into());
}

pub fn set_optional_string_property(
    scope: &mut PinScope<'_, '_>,
    object: Local<'_, Object>,
    key: &str,
    value: Option<&str>,
) {
    let value = match value {
        Some(value) => match v8_string(scope, value) {
            Some(value) => value.into(),
            None => v8::null(scope).into(),
        },
        None => v8::null(scope).into(),
    };
    set_property(scope, object, key, value);
}

pub fn set_number_property(
    scope: &mut PinScope<'_, '_>,
    object: Local<'_, Object>,
    key: &str,
    value: f64,
) {
    let value = v8::Number::new(scope, value);
    set_property(scope, object, key, value.into());
}

pub fn define_non_enumerable_property(
    scope: &mut PinScope<'_, '_>,
    object: Local<'_, Object>,
    key: &str,
    value: Local<'_, Value>,
) {
    let Some(key) = v8_string(scope, key) else {
        return;
    };
    let _ = object.define_own_property(scope, key.into(), value, PropertyAttribute::DONT_ENUM);
}

pub fn define_non_enumerable_static_property(
    scope: &mut PinScope<'_, '_>,
    object: Local<'_, Object>,
    key: &'static str,
    value: Local<'_, Value>,
) {
    let _ = object.define_own_property(
        scope,
        v8str(scope, key).into(),
        value,
        PropertyAttribute::DONT_ENUM,
    );
}

pub fn define_non_enumerable_static_string_property(
    scope: &mut PinScope<'_, '_>,
    object: Local<'_, Object>,
    key: &'static str,
    value: &str,
) {
    let Some(value) = v8_string(scope, value) else {
        return;
    };
    define_non_enumerable_static_property(scope, object, key, value.into());
}

pub fn define_non_enumerable_static_bool_property(
    scope: &mut PinScope<'_, '_>,
    object: Local<'_, Object>,
    key: &'static str,
    value: bool,
) {
    let value = v8::Boolean::new(scope, value);
    define_non_enumerable_static_property(scope, object, key, value.into());
}

pub fn define_non_enumerable_static_number_property(
    scope: &mut PinScope<'_, '_>,
    object: Local<'_, Object>,
    key: &'static str,
    value: f64,
) {
    let value = v8::Number::new(scope, value);
    define_non_enumerable_static_property(scope, object, key, value.into());
}
