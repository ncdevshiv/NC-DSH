use crate::selector::SelectorError;

use super::*;
use crate::native_bridge::abort::dom_exception_value;

pub(super) fn throw_named_error(
    scope: &mut v8::PinScope<'_, '_>,
    name: &str,
    message: &str,
    code: Option<u16>,
) {
    let Some(message_value) = v8_string(scope, message) else {
        return;
    };
    let exception = v8::Exception::error(scope, message_value);
    if let Some(object) = exception.to_object(scope) {
        if let Some(name_value) = v8_string(scope, name) {
            let _ = object.set(scope, v8str(scope, "name").into(), name_value.into());
        }
        if let Some(code) = code {
            let _ = object.set(
                scope,
                v8str(scope, "code").into(),
                v8::Number::new(scope, f64::from(code)).into(),
            );
        }
    }
    scope.throw_exception(exception);
}

pub(in crate::native_bridge) fn throw_native_selector_error(
    scope: &mut v8::PinScope<'_, '_>,
    error: &SelectorError,
) {
    let exception = dom_exception_value(scope, error.message(), "SyntaxError");
    scope.throw_exception(exception);
}

pub(in crate::native_bridge) fn throw_native_selector_error_for_selector(
    scope: &mut v8::PinScope<'_, '_>,
    _selector: &str,
    error: &SelectorError,
) {
    throw_native_selector_error(scope, error);
}
