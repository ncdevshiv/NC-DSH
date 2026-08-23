use super::super::util::v8str;
use super::definition::CustomElementDefineError;
use crate::webidl;

pub(super) fn sequence_property_for_constructor<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    constructor: v8::Local<'s, v8::Function>,
    property_name: &'static str,
) -> Result<Vec<String>, CustomElementDefineError> {
    let property = constructor
        .get(scope, v8str(scope, property_name).into())
        .ok_or(CustomElementDefineError::PendingException)?;
    sequence_from_value(scope, property, property_name)
}

fn sequence_from_value<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    value: v8::Local<'s, v8::Value>,
    property_name: &'static str,
) -> Result<Vec<String>, CustomElementDefineError> {
    if value.is_undefined() {
        return Ok(Vec::new());
    }
    let sequence = webidl::convert::<webidl::Sequence<webidl::DomString>>(
        scope,
        value,
        webidl::Context::member("CustomElementRegistry.define", property_name),
    )
    .map_err(|error| {
        if error.is_pending_exception() {
            CustomElementDefineError::PendingException
        } else {
            CustomElementDefineError::InvalidSequence(property_name)
        }
    })?;
    Ok(sequence.0.into_iter().map(|value| value.0).collect())
}
