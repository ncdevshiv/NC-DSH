use crate::webidl;

pub(super) fn parse_transaction_store_names<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    value: v8::Local<'s, v8::Value>,
) -> Result<Vec<String>, webidl::WebIdlError> {
    if should_parse_store_names_sequence(scope, value)? {
        let names = webidl::convert::<webidl::Sequence<webidl::DomString>>(
            scope,
            value,
            webidl::Context::argument("IDBDatabase.transaction", 1),
        )?;
        return Ok(names.0.into_iter().map(Into::into).collect());
    }
    webidl::convert::<webidl::DomString>(
        scope,
        value,
        webidl::Context::argument("IDBDatabase.transaction", 1),
    )
    .map(|value| vec![value.into()])
}

fn should_parse_store_names_sequence<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    value: v8::Local<'s, v8::Value>,
) -> Result<bool, webidl::WebIdlError> {
    if value.is_string() {
        return Ok(false);
    }
    let Ok(object) = v8::Local::<v8::Object>::try_from(value) else {
        return Ok(false);
    };
    let iterator_key = v8::Symbol::get_iterator(scope);
    let Some(iterator) = webidl::symbol_property_result(
        scope,
        object,
        iterator_key,
        webidl::Context::argument("IDBDatabase.transaction", 1),
    )?
    else {
        return Ok(false);
    };
    Ok(!iterator.is_null_or_undefined())
}
