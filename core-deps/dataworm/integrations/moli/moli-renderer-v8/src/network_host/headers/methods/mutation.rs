use super::super::store::{
    HeadersGuard, header_allowed_by_guard, headers_are_immutable, headers_entries, headers_guard,
    normalized_header_name_or_throw, normalized_header_value_or_throw, normalized_headers_entries,
    set_headers_entries,
};
use super::*;
use crate::webidl;

#[derive(webidl::WebIdlArgs)]
#[webidl(prefix = "Headers")]
struct HeadersNameValueArgs {
    #[webidl(required, converter = "byte_string")]
    name: String,
    #[webidl(required, converter = "byte_string")]
    value: String,
}

#[derive(webidl::WebIdlArgs)]
#[webidl(prefix = "Headers")]
struct HeadersNameArgs {
    #[webidl(required, converter = "byte_string")]
    name: String,
}

fn reject_immutable_headers<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    headers: v8::Local<'s, v8::Object>,
) -> bool {
    if !headers_are_immutable(scope, headers) {
        return false;
    }
    throw_type_error(scope, "Headers are immutable");
    true
}

fn candidate_allowed_by_guard(
    guard: HeadersGuard,
    target_name: &str,
    candidate_entries: &[(String, String)],
) -> bool {
    if guard != HeadersGuard::RequestNoCors {
        return true;
    }
    normalized_headers_entries(candidate_entries)
        .into_iter()
        .filter(|(name, _)| name == target_name)
        .all(|(name, value)| header_allowed_by_guard(guard, &name, &value))
}

pub(in crate::network_host::headers) fn headers_set_callback<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    _rv: v8::ReturnValue<'_, v8::Value>,
) {
    let Some(this) = require_headers_receiver(scope, args.this()) else {
        return;
    };
    if reject_immutable_headers(scope, this) {
        return;
    }
    let Some(parsed) = webidl::parse_args::<HeadersNameValueArgs>(scope, &args) else {
        return;
    };
    let name = parsed.name;
    let value = parsed.value;
    let Some(lower) = normalized_header_name_or_throw(scope, &name) else {
        return;
    };
    let Some(value) = normalized_header_value_or_throw(scope, &value) else {
        return;
    };
    let guard = headers_guard(scope, this);
    if !header_allowed_by_guard(guard, &lower, &value) {
        return;
    }
    let mut entries = headers_entries(scope, this);
    let insert_at = entries
        .iter()
        .position(|(entry_name, _)| *entry_name == lower)
        .unwrap_or(entries.len());
    entries.retain(|(entry_name, _)| *entry_name != lower);
    let target_name = lower.clone();
    entries.insert(insert_at.min(entries.len()), (lower, value));
    if !candidate_allowed_by_guard(guard, &target_name, &entries) {
        return;
    }
    set_headers_entries(scope, this, &entries);
}

pub(in crate::network_host::headers) fn headers_delete_callback<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    _rv: v8::ReturnValue<'_, v8::Value>,
) {
    let Some(this) = require_headers_receiver(scope, args.this()) else {
        return;
    };
    if reject_immutable_headers(scope, this) {
        return;
    }
    let Some(parsed) = webidl::parse_args::<HeadersNameArgs>(scope, &args) else {
        return;
    };
    let name = parsed.name;
    let Some(lower) = normalized_header_name_or_throw(scope, &name) else {
        return;
    };
    let mut entries = headers_entries(scope, this);
    entries.retain(|(entry_name, _)| *entry_name != lower);
    set_headers_entries(scope, this, &entries);
}

pub(in crate::network_host::headers) fn headers_append_callback<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    _rv: v8::ReturnValue<'_, v8::Value>,
) {
    let Some(this) = require_headers_receiver(scope, args.this()) else {
        return;
    };
    if reject_immutable_headers(scope, this) {
        return;
    }
    let Some(parsed) = webidl::parse_args::<HeadersNameValueArgs>(scope, &args) else {
        return;
    };
    let name = parsed.name;
    let value = parsed.value;
    let Some(name) = normalized_header_name_or_throw(scope, &name) else {
        return;
    };
    let Some(value) = normalized_header_value_or_throw(scope, &value) else {
        return;
    };
    let guard = headers_guard(scope, this);
    if !header_allowed_by_guard(guard, &name, &value) {
        return;
    }
    let mut entries = headers_entries(scope, this);
    entries.push((name.clone(), value));
    if !candidate_allowed_by_guard(guard, &name, &entries) {
        return;
    }
    set_headers_entries(scope, this, &entries);
}
