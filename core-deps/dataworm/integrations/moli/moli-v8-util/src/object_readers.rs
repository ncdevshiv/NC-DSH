use v8::{Array, Function, Local, Object, PinScope, Value};

use crate::properties::{get_own_property, get_own_static_property, get_property};

pub fn object_string_property(
    scope: &mut PinScope<'_, '_>,
    object: Local<'_, Object>,
    key: &str,
) -> Option<String> {
    get_property(scope, object, key)
        .and_then(|value| value.to_string(scope))
        .map(|value| value.to_rust_string_lossy(scope))
}

pub fn object_own_static_string_property(
    scope: &mut PinScope<'_, '_>,
    object: Local<'_, Object>,
    key: &'static str,
) -> Option<String> {
    get_own_static_property(scope, object, key)
        .and_then(|value| value.to_string(scope))
        .map(|value| value.to_rust_string_lossy(scope))
}

pub fn object_own_string_property(
    scope: &mut PinScope<'_, '_>,
    object: Local<'_, Object>,
    key: &str,
) -> Option<String> {
    get_own_property(scope, object, key)
        .and_then(|value| value.to_string(scope))
        .map(|value| value.to_rust_string_lossy(scope))
}

pub fn object_defined_string_property(
    scope: &mut PinScope<'_, '_>,
    object: Local<'_, Object>,
    key: &str,
) -> Option<String> {
    let value = get_property(scope, object, key)?;
    if value.is_null_or_undefined() {
        return None;
    }
    value
        .to_string(scope)
        .map(|value| value.to_rust_string_lossy(scope))
}

pub fn object_non_empty_string_property(
    scope: &mut PinScope<'_, '_>,
    object: Local<'_, Object>,
    key: &str,
) -> Option<String> {
    object_defined_string_property(scope, object, key).filter(|value| !value.is_empty())
}

pub fn object_number_property(
    scope: &mut PinScope<'_, '_>,
    object: Local<'_, Object>,
    key: &str,
) -> Option<f64> {
    get_property(scope, object, key)?.number_value(scope)
}

pub fn object_own_static_number_property(
    scope: &mut PinScope<'_, '_>,
    object: Local<'_, Object>,
    key: &'static str,
) -> Option<f64> {
    get_own_static_property(scope, object, key)?.number_value(scope)
}

pub fn object_own_number_property(
    scope: &mut PinScope<'_, '_>,
    object: Local<'_, Object>,
    key: &str,
) -> Option<f64> {
    get_own_property(scope, object, key)?.number_value(scope)
}

pub fn object_bool_property(
    scope: &mut PinScope<'_, '_>,
    object: Local<'_, Object>,
    key: &str,
) -> Option<bool> {
    get_property(scope, object, key).map(|value| value.boolean_value(scope))
}

pub fn object_own_static_bool_property(
    scope: &mut PinScope<'_, '_>,
    object: Local<'_, Object>,
    key: &'static str,
) -> Option<bool> {
    get_own_static_property(scope, object, key).map(|value| value.boolean_value(scope))
}

pub fn object_own_bool_property(
    scope: &mut PinScope<'_, '_>,
    object: Local<'_, Object>,
    key: &str,
) -> Option<bool> {
    get_own_property(scope, object, key).map(|value| value.boolean_value(scope))
}

pub fn object_property_as_object<'s>(
    scope: &mut PinScope<'s, '_>,
    object: Local<'_, Object>,
    key: &str,
) -> Option<Local<'s, Object>> {
    get_property(scope, object, key).and_then(|value| Local::<Object>::try_from(value).ok())
}

pub fn object_own_static_property_as_object<'s>(
    scope: &mut PinScope<'s, '_>,
    object: Local<'_, Object>,
    key: &'static str,
) -> Option<Local<'s, Object>> {
    get_own_static_property(scope, object, key)
        .and_then(|value| Local::<Object>::try_from(value).ok())
}

pub fn object_own_property_as_object<'s>(
    scope: &mut PinScope<'s, '_>,
    object: Local<'_, Object>,
    key: &str,
) -> Option<Local<'s, Object>> {
    get_own_property(scope, object, key).and_then(|value| Local::<Object>::try_from(value).ok())
}

pub fn walk_object_chain<'s>(
    scope: &mut PinScope<'s, '_>,
    start: Local<'s, Object>,
    property: &str,
) -> Vec<Local<'s, Object>> {
    let mut chain = vec![start];
    let mut current = start;
    while let Some(next) = object_property_as_object(scope, current, property) {
        chain.push(next);
        current = next;
    }
    chain
}

pub fn walk_object_chain_last<'s>(
    scope: &mut PinScope<'s, '_>,
    start: Local<'s, Object>,
    property: &str,
) -> Local<'s, Object> {
    let mut current = start;
    while let Some(next) = object_property_as_object(scope, current, property) {
        current = next;
    }
    current
}

pub fn object_chain_contains<'s>(chain: &[Local<'s, Object>], target: Local<'s, Object>) -> bool {
    chain.iter().any(|node| node.strict_equals(target.into()))
}

pub fn object_property_as_array<'s>(
    scope: &mut PinScope<'s, '_>,
    object: Local<'_, Object>,
    key: &str,
) -> Option<Local<'s, Array>> {
    get_property(scope, object, key).and_then(|value| Local::<Array>::try_from(value).ok())
}

pub fn object_own_static_property_as_array<'s>(
    scope: &mut PinScope<'s, '_>,
    object: Local<'_, Object>,
    key: &'static str,
) -> Option<Local<'s, Array>> {
    get_own_static_property(scope, object, key)
        .and_then(|value| Local::<Array>::try_from(value).ok())
}

pub fn object_own_property_as_array<'s>(
    scope: &mut PinScope<'s, '_>,
    object: Local<'_, Object>,
    key: &str,
) -> Option<Local<'s, Array>> {
    get_own_property(scope, object, key).and_then(|value| Local::<Array>::try_from(value).ok())
}

pub fn call_object_method<'s>(
    scope: &mut PinScope<'s, '_>,
    object: Local<'s, Object>,
    name: &str,
    args: &[Local<'s, Value>],
) -> Option<Local<'s, Value>> {
    let function = get_property(scope, object, name)?;
    let function = Local::<Function>::try_from(function).ok()?;
    function.call(scope, object.into(), args)
}
