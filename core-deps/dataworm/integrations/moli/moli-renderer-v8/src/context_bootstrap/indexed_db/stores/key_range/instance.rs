use super::*;

#[derive(webidl::WebIdlArgs)]
#[webidl(prefix = "IDBKeyRange.includes")]
struct IdbKeyRangeIncludesArgs<'s> {
    #[webidl(required, converter = "raw")]
    key: v8::Local<'s, v8::Value>,
}

pub(in crate::context_bootstrap::indexed_db) fn idb_key_range_includes_callback<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue<'s, v8::Value>,
) {
    let Some(parsed) = webidl::parse_args::<IdbKeyRangeIncludesArgs<'s>>(scope, &args) else {
        return;
    };
    let Some(range) = parse_key_range_from_value(scope, args.this().into()) else {
        rv.set_bool(false);
        return;
    };
    let key = match parse_idb_key(scope, parsed.key) {
        Ok(Some(key)) => key,
        _ => {
            let error = dom_exception_value(
                scope,
                "Failed to execute 'includes': the key is not a valid key.",
                "DataError",
            );
            scope.throw_exception(error);
            return;
        }
    };
    rv.set_bool(key_in_range(&key, &range));
}
