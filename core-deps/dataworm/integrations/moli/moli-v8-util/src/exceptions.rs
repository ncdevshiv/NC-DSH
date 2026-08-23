use moli_web_errors::dom_exception_legacy_code;
use v8::{Exception, Function, Local, PinScope};

use crate::strings::{v8_string, v8str};

pub fn throw_type_error(scope: &mut PinScope<'_, '_>, message: &str) {
    let Some(message) = v8_string(scope, message) else {
        return;
    };
    scope.throw_exception(Exception::type_error(scope, message));
}

pub fn throw_range_error(scope: &mut PinScope<'_, '_>, message: &str) {
    let Some(message) = v8_string(scope, message) else {
        return;
    };
    scope.throw_exception(Exception::range_error(scope, message));
}

pub fn throw_error(scope: &mut PinScope<'_, '_>, message: &str) {
    let Some(message) = v8_string(scope, message) else {
        return;
    };
    scope.throw_exception(Exception::error(scope, message));
}

pub fn dom_exception_value<'s>(
    scope: &mut PinScope<'s, '_>,
    name: &'static str,
    message: &str,
) -> Local<'s, v8::Value> {
    let message_value = v8_string(scope, message).unwrap_or_else(|| v8::String::empty(scope));
    let global = scope.get_current_context().global(scope);
    if let Some(constructor_value) = global.get(scope, v8str(scope, "DOMException").into())
        && let Ok(constructor) = Local::<Function>::try_from(constructor_value)
    {
        let args: [Local<'_, v8::Value>; 2] = [message_value.into(), v8str(scope, name).into()];
        if let Some(exception) = constructor.new_instance(scope, &args) {
            return exception.into();
        }
    }
    let fallback = Exception::error(scope, message_value);
    if let Some(object) = fallback.to_object(scope) {
        let name_value = v8str(scope, name);
        let code_value =
            v8::Integer::new_from_unsigned(scope, u32::from(dom_exception_legacy_code(name)));
        let _ = object.set(scope, v8str(scope, "name").into(), name_value.into());
        let _ = object.set(scope, v8str(scope, "code").into(), code_value.into());
    }
    fallback
}

pub fn throw_dom_exception(scope: &mut PinScope<'_, '_>, name: &'static str, message: &str) {
    let exception = dom_exception_value(scope, name, message);
    scope.throw_exception(exception);
}

pub fn throw_index_size_error(scope: &mut PinScope<'_, '_>) {
    throw_dom_exception(
        scope,
        "IndexSizeError",
        "Index or size is negative or greater than the allowed amount.",
    );
}
