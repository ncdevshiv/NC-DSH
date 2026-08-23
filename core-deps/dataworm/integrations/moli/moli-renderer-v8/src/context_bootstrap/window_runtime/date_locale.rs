use super::*;
use crate::util::{get_private_value, set_private_value};
use moli_time::{DateLocaleFormatKind, format_date_locale_value};

const DATE_LOCALE_OVERRIDE_SLOT: &str = "__moliLocaleOverride";
const DATE_TIMEZONE_OVERRIDE_SLOT: &str = "__moliTimeZoneOverride";

pub(crate) fn set_date_locale_override_for_current_context<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    locale: Option<&str>,
) {
    let global = scope.get_current_context().global(scope);
    set_date_override_private_value(scope, global, DATE_LOCALE_OVERRIDE_SLOT, locale);
}

pub(crate) fn set_date_timezone_override_for_current_context<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    timezone: Option<&str>,
) {
    let global = scope.get_current_context().global(scope);
    set_date_override_private_value(scope, global, DATE_TIMEZONE_OVERRIDE_SLOT, timezone);
}

pub(in crate::context_bootstrap) fn current_date_locale_overrides<'s>(
    scope: &mut v8::PinScope<'s, '_>,
) -> (Option<String>, Option<String>) {
    let (mut locale_override, mut timezone_override) = context_host_ptr_from_global_bridge(scope)
        .map(|host_ptr| unsafe {
            let host = &*host_ptr;
            (
                host.locale_override().map(str::to_owned),
                host.timezone_override().map(str::to_owned),
            )
        })
        .unwrap_or((None, None));
    if locale_override.is_none() || timezone_override.is_none() {
        let global = scope.get_current_context().global(scope);
        if locale_override.is_none() {
            locale_override = date_override_private_value(scope, global, DATE_LOCALE_OVERRIDE_SLOT);
        }
        if timezone_override.is_none() {
            timezone_override =
                date_override_private_value(scope, global, DATE_TIMEZONE_OVERRIDE_SLOT);
        }
    }
    (locale_override, timezone_override)
}

pub(in crate::context_bootstrap) fn date_to_locale_string_callback(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments<'_>,
    mut rv: v8::ReturnValue<'_, v8::Value>,
) {
    set_date_locale_result(scope, &args, &mut rv, DateLocaleFormatKind::DateTime);
}

pub(in crate::context_bootstrap) fn date_to_locale_date_string_callback(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments<'_>,
    mut rv: v8::ReturnValue<'_, v8::Value>,
) {
    set_date_locale_result(scope, &args, &mut rv, DateLocaleFormatKind::DateOnly);
}

pub(in crate::context_bootstrap) fn date_to_locale_time_string_callback(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments<'_>,
    mut rv: v8::ReturnValue<'_, v8::Value>,
) {
    set_date_locale_result(scope, &args, &mut rv, DateLocaleFormatKind::TimeOnly);
}

fn set_date_locale_result(
    scope: &mut v8::PinScope<'_, '_>,
    args: &v8::FunctionCallbackArguments<'_>,
    rv: &mut v8::ReturnValue<'_, v8::Value>,
    kind: DateLocaleFormatKind,
) {
    let Ok(date) = v8::Local::<v8::Date>::try_from(args.this()) else {
        throw_type_error(
            scope,
            "Method Date.prototype.toLocaleString called on incompatible receiver.",
        );
        return;
    };
    let (locale_override, timezone_override) = current_date_locale_overrides(scope);
    let formatted = format_date_locale_value(
        date.value_of(),
        kind,
        locale_override.as_deref(),
        timezone_override.as_deref(),
    );
    if let Some(value) = v8_string(scope, &formatted) {
        rv.set(value.into());
    } else {
        rv.set(v8::String::empty(scope).into());
    }
}

fn set_date_override_private_value<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    global: v8::Local<'s, v8::Object>,
    slot: &'static str,
    value: Option<&str>,
) {
    let value = value
        .and_then(|value| v8_string(scope, value).map(Into::into))
        .unwrap_or_else(|| v8::undefined(scope).into());
    set_private_value(scope, global, slot, value);
}

fn date_override_private_value<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    global: v8::Local<'s, v8::Object>,
    slot: &'static str,
) -> Option<String> {
    get_private_value(scope, global, slot)
        .and_then(|value| value.to_string(scope))
        .map(|value| value.to_rust_string_lossy(scope))
        .filter(|value| !value.is_empty() && value != "null" && value != "undefined")
}
