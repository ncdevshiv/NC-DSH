use crate::util::{v8_string, v8str};

use super::super::super::super::callback_arg_string;

enum HtmlAllNameOrIndex {
    Index(u32),
    Name(String),
}

fn array_index_property_name(value: &str) -> Option<u32> {
    if value.is_empty() || (value.len() > 1 && value.starts_with('0')) {
        return None;
    }
    if !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let index = value.parse::<u64>().ok()?;
    if index >= u64::from(u32::MAX) {
        return None;
    }
    u32::try_from(index).ok()
}

fn document_all_name_or_index(
    scope: &mut v8::PinScope<'_, '_>,
    args: &v8::FunctionCallbackArguments<'_>,
) -> Option<HtmlAllNameOrIndex> {
    if args.length() == 0 || args.get(0).is_undefined() {
        return None;
    }
    let name_or_index = callback_arg_string(scope, args, 0)?;
    Some(match array_index_property_name(&name_or_index) {
        Some(index) => HtmlAllNameOrIndex::Index(index),
        None => HtmlAllNameOrIndex::Name(name_or_index),
    })
}

fn resolve_document_all_value<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    items: v8::Local<'s, v8::Array>,
    named: v8::Local<'s, v8::Object>,
    name_or_index: HtmlAllNameOrIndex,
) -> Option<v8::Local<'s, v8::Value>> {
    match name_or_index {
        HtmlAllNameOrIndex::Index(index) => items
            .get_index(scope, index)
            .filter(|value| !value.is_null_or_undefined()),
        HtmlAllNameOrIndex::Name(key) => {
            let key = v8_string(scope, &key)?;
            named
                .get(scope, key.into())
                .filter(|value| !value.is_null_or_undefined())
        }
    }
}

pub(super) fn document_all_call_callback(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments<'_>,
    mut rv: v8::ReturnValue<'_, v8::Value>,
) {
    let Ok(data) = v8::Local::<v8::Object>::try_from(args.data()) else {
        rv.set_null();
        return;
    };
    let Some(items) = data
        .get(scope, v8str(scope, "items").into())
        .and_then(|value| v8::Local::<v8::Array>::try_from(value).ok())
    else {
        rv.set_null();
        return;
    };
    let Some(named) = data
        .get(scope, v8str(scope, "named").into())
        .and_then(|value| v8::Local::<v8::Object>::try_from(value).ok())
    else {
        rv.set_null();
        return;
    };
    let Some(name_or_index) = document_all_name_or_index(scope, &args) else {
        rv.set_null();
        return;
    };
    match resolve_document_all_value(scope, items, named, name_or_index) {
        Some(value) => rv.set(value),
        None => rv.set_null(),
    }
}

pub(super) fn document_all_item_callback(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments<'_>,
    mut rv: v8::ReturnValue<'_, v8::Value>,
) {
    let Ok(data) = v8::Local::<v8::Object>::try_from(args.data()) else {
        rv.set_null();
        return;
    };
    let Some(items) = data
        .get(scope, v8str(scope, "items").into())
        .and_then(|value| v8::Local::<v8::Array>::try_from(value).ok())
    else {
        rv.set_null();
        return;
    };
    let Some(named) = data
        .get(scope, v8str(scope, "named").into())
        .and_then(|value| v8::Local::<v8::Object>::try_from(value).ok())
    else {
        rv.set_null();
        return;
    };
    let Some(name_or_index) = document_all_name_or_index(scope, &args) else {
        rv.set_null();
        return;
    };
    match resolve_document_all_value(scope, items, named, name_or_index) {
        Some(value) => rv.set(value),
        None => rv.set_null(),
    }
}

pub(super) fn document_all_named_item_callback(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments<'_>,
    mut rv: v8::ReturnValue<'_, v8::Value>,
) {
    let Ok(data) = v8::Local::<v8::Object>::try_from(args.data()) else {
        rv.set_null();
        return;
    };
    let Some(named) = data
        .get(scope, v8str(scope, "named").into())
        .and_then(|value| v8::Local::<v8::Object>::try_from(value).ok())
    else {
        rv.set_null();
        return;
    };
    let Some(key) = callback_arg_string(scope, &args, 0) else {
        rv.set_null();
        return;
    };
    let Some(key) = v8_string(scope, &key) else {
        rv.set_null();
        return;
    };
    match named.get(scope, key.into()) {
        Some(value) if !value.is_null_or_undefined() => rv.set(value),
        _ => rv.set_null(),
    }
}
