use super::*;

#[derive(webidl::WebIdlArgs)]
#[webidl(prefix = "IDBCursor.update")]
struct IdbCursorUpdateArgs<'s> {
    #[webidl(required, converter = "raw")]
    value: v8::Local<'s, v8::Value>,
}

fn cursor_entry_update_value<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    cursor: v8::Local<'s, v8::Object>,
    position: usize,
    value: v8::Local<'s, v8::Value>,
) -> Option<()> {
    let entry = cursor_entry_object(scope, cursor, position)?;
    define_public_non_enumerable_value_property(scope, entry, "value", value);
    if !object_bool_property(scope, cursor, INDEXED_DB_CURSOR_KEY_ONLY_SLOT).unwrap_or(false) {
        let _ = cursor.set(scope, v8str(scope, "value").into(), value);
    }
    Some(())
}

fn cursor_update_preserves_primary_key<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    cursor: v8::Local<'s, v8::Object>,
    value: v8::Local<'s, v8::Value>,
    primary_key: &Key,
) -> bool {
    let Some(store) = cursor_store_object(scope, cursor) else {
        return true;
    };
    let Some(key_path) = store
        .get(scope, v8str(scope, "keyPath").into())
        .and_then(|value| key_path_from_js_value(scope, value))
    else {
        return true;
    };
    let Some(derived_key) = extract_index_keys_from_value(scope, value, &key_path, false)
        .into_iter()
        .next()
    else {
        return false;
    };
    &derived_key == primary_key
}

pub(in crate::context_bootstrap::indexed_db) fn idb_cursor_update_callback<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue<'s, v8::Value>,
) {
    let Some(parsed) = webidl::parse_args::<IdbCursorUpdateArgs<'s>>(scope, &args) else {
        return;
    };
    let cursor = args.this();
    if object_bool_property(scope, cursor, INDEXED_DB_CURSOR_KEY_ONLY_SLOT).unwrap_or(false) {
        let error = dom_exception_value(
            scope,
            "The cursor does not expose values.",
            "InvalidStateError",
        );
        scope.throw_exception(error);
        return;
    }
    let Some(position) = cursor_current_position(scope, cursor) else {
        let error = dom_exception_value(scope, "The cursor is exhausted.", "InvalidStateError");
        scope.throw_exception(error);
        return;
    };
    let Some((request, handle, store_name)) = create_cursor_request(scope, cursor) else {
        let error = dom_exception_value(
            scope,
            "The transaction is not active.",
            "TransactionInactiveError",
        );
        scope.throw_exception(error);
        return;
    };
    let Some(value_bytes) = serialize_js_value(scope, parsed.value) else {
        return;
    };
    let Some(primary_key) = cursor_primary_key_at(scope, cursor, position) else {
        let error = dom_exception_value(scope, "The cursor is exhausted.", "InvalidStateError");
        scope.throw_exception(error);
        return;
    };
    if !cursor_update_preserves_primary_key(scope, cursor, parsed.value, &primary_key) {
        let error = dom_exception_value(
            scope,
            "Failed to execute 'update': the value changes the effective key.",
            "DataError",
        );
        scope.throw_exception(error);
        return;
    }
    let quota_check = match cursor_store_object(scope, cursor)
        .and_then(|store| storage_bucket_quota_check_for_object_store(scope, store))
    {
        Some(Ok(quota)) => Some(quota),
        Some(Err(error)) => {
            let error = request_error_object(scope, &error);
            store_request_error(scope, request, error);
            rv.set(request.into());
            return;
        }
        None => None,
    };
    match with_indexed_db_manager(scope, |manager| {
        if let Some(quota) = quota_check {
            manager.put_with_quota(
                handle,
                &store_name,
                Some(primary_key.clone()),
                value_bytes,
                quota.quota_check,
            )
        } else {
            manager.put(handle, &store_name, Some(primary_key.clone()), value_bytes)
        }
    }) {
        Ok(key) => {
            let _ = cursor_entry_update_value(scope, cursor, position, parsed.value);
            let key = key_to_js_value(scope, &key);
            store_request_success(scope, request, key);
        }
        Err(error) => {
            let error = request_error_object(scope, &error);
            store_request_error(scope, request, error);
        }
    }
    rv.set(request.into());
}
