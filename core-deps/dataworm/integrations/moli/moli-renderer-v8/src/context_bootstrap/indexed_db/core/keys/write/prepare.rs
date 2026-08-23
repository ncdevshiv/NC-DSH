use super::*;

pub(in crate::context_bootstrap::indexed_db) fn prepare_object_store_write<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    store: v8::Local<'s, v8::Object>,
    handle: TransactionHandle,
    store_name: &str,
    value: v8::Local<'s, v8::Value>,
    explicit_key: Option<Key>,
) -> Result<PreparedObjectStoreWrite<'s>, PreparedObjectStoreWriteError> {
    let key_path = store
        .get(scope, v8str(scope, "keyPath").into())
        .and_then(|value| key_path_from_js_value(scope, value));
    let auto_increment = object_bool_property(scope, store, "autoIncrement").unwrap_or(false);
    if key_path.is_some() && explicit_key.is_some() {
        return Err(PreparedObjectStoreWriteError::DomException {
            message: "Failed to execute the operation: the store uses in-line keys and does not accept a key argument.",
            name: "DataError",
        });
    }
    match key_path {
        Some(key_path) => {
            if let Some(key) = derive_object_store_key_from_value(scope, store, value) {
                return Ok(PreparedObjectStoreWrite {
                    key: Some(key),
                    value,
                });
            }
            if !auto_increment {
                return Err(PreparedObjectStoreWriteError::DomException {
                    message: "Failed to execute the operation: the value does not contain the object store keyPath.",
                    name: "DataError",
                });
            }
            let generated_key =
                with_indexed_db_manager(scope, |manager| manager.generate_key(handle, store_name))
                    .map_err(PreparedObjectStoreWriteError::Backend)?;
            let KeyPath::String(key_path) = key_path else {
                return Err(PreparedObjectStoreWriteError::DomException {
                    message: "Failed to execute the operation: compound keyPath stores cannot accept generated inline keys.",
                    name: "InvalidAccessError",
                });
            };
            let value = inject_key_path_into_value(scope, value, &key_path, &generated_key)?;
            Ok(PreparedObjectStoreWrite {
                key: Some(generated_key),
                value,
            })
        }
        None => {
            if explicit_key.is_none() && !auto_increment {
                return Err(PreparedObjectStoreWriteError::DomException {
                    message: "Failed to execute the operation: a key is required for stores without autoIncrement.",
                    name: "DataError",
                });
            }
            let key = if let Some(key) = explicit_key {
                Some(key)
            } else if auto_increment {
                Some(
                    with_indexed_db_manager(scope, |manager| {
                        manager.generate_key(handle, store_name)
                    })
                    .map_err(PreparedObjectStoreWriteError::Backend)?,
                )
            } else {
                None
            };
            Ok(PreparedObjectStoreWrite { key, value })
        }
    }
}
