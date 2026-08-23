use super::*;
use moli_webapi_declare::ObjectLiteralDeclaration;

pub(super) fn inject_key_path_into_value<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    value: v8::Local<'s, v8::Value>,
    key_path: &str,
    key: &Key,
) -> Result<v8::Local<'s, v8::Value>, PreparedObjectStoreWriteError> {
    let cloned =
        clone_js_value(scope, value).ok_or(PreparedObjectStoreWriteError::DomException {
            message: "Failed to clone the value before assigning the generated key.",
            name: "DataCloneError",
        })?;
    let mut current = v8::Local::<v8::Object>::try_from(cloned).map_err(|_| {
        PreparedObjectStoreWriteError::DomException {
            message: "Failed to execute the operation: the value cannot accept an inline key.",
            name: "DataError",
        }
    })?;
    let mut segments = key_path.split('.').peekable();
    while let Some(segment) = segments.next() {
        let property = v8_string(scope, segment).unwrap_or_else(|| v8::String::empty(scope));
        if segments.peek().is_none() {
            let key_value = key_to_js_value(scope, key);
            let _ = current.set(scope, property.into(), key_value);
            return Ok(cloned);
        }
        let next = current.get(scope, property.into());
        let next_object = match next {
            Some(next) if next.is_undefined() => {
                let nested = generated_key_path_suffix_object(scope, &mut segments, key);
                let _ = current.set(scope, property.into(), nested.into());
                return Ok(cloned);
            }
            Some(next) => v8::Local::<v8::Object>::try_from(next).map_err(|_| {
                PreparedObjectStoreWriteError::DomException {
                    message:
                        "Failed to execute the operation: the value cannot accept an inline key.",
                    name: "DataError",
                }
            })?,
            None => {
                return Err(PreparedObjectStoreWriteError::DomException {
                    message:
                        "Failed to execute the operation: the value cannot accept an inline key.",
                    name: "DataError",
                });
            }
        };
        current = next_object;
    }
    Err(PreparedObjectStoreWriteError::DomException {
        message: "Failed to execute the operation: keyPath is empty.",
        name: "DataError",
    })
}

fn generated_key_path_suffix_object<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    segments: &mut std::iter::Peekable<std::str::Split<'_, char>>,
    key: &Key,
) -> v8::Local<'s, v8::Object> {
    let segment = segments
        .next()
        .expect("generated key path suffix should include a segment");
    let property = v8_string(scope, segment).unwrap_or_else(|| v8::String::empty(scope));
    let value = if segments.peek().is_none() {
        key_to_js_value(scope, key)
    } else {
        generated_key_path_suffix_object(scope, segments, key).into()
    };
    let object = ObjectLiteralDeclaration::bind(scope);
    object.set_value_property(scope, property.into(), value);
    object.into_object()
}
