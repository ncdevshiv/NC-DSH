use super::*;

pub(in crate::context_bootstrap::indexed_db) fn transaction_db_key(
    scope: &mut v8::PinScope<'_, '_>,
    transaction: v8::Local<'_, v8::Object>,
) -> Option<String> {
    object_string_property(scope, transaction, INDEXED_DB_TRANSACTION_DB_KEY_SLOT)
}

pub(in crate::context_bootstrap::indexed_db) fn register_readwrite_transaction(
    scope: &mut v8::PinScope<'_, '_>,
    transaction: v8::Local<'_, v8::Object>,
) {
    push_unique_object_to_indexed_db_runtime_array(
        scope,
        IndexedDbRuntimeArray::ReadwriteTransactions,
        transaction,
    );
}

pub(in crate::context_bootstrap::indexed_db) fn unregister_readwrite_transaction(
    scope: &mut v8::PinScope<'_, '_>,
    transaction: v8::Local<'_, v8::Object>,
) {
    let Some(queue) = indexed_db_runtime_array(scope, IndexedDbRuntimeArray::ReadwriteTransactions)
    else {
        return;
    };
    let next = v8::Array::new(scope, 0);
    for index in 0..queue.length() {
        let Some(value) = queue.get_index(scope, index) else {
            continue;
        };
        if value.strict_equals(transaction.into()) {
            continue;
        }
        let _ = next.set_index(scope, next.length(), value);
    }
    replace_indexed_db_runtime_array(scope, IndexedDbRuntimeArray::ReadwriteTransactions, next);
}

pub(in crate::context_bootstrap::indexed_db) fn readwrite_transaction_can_start(
    scope: &mut v8::PinScope<'_, '_>,
    transaction: v8::Local<'_, v8::Object>,
) -> bool {
    let Some(db_key) = transaction_db_key(scope, transaction) else {
        return true;
    };
    let Some(queue) = indexed_db_runtime_array(scope, IndexedDbRuntimeArray::ReadwriteTransactions)
    else {
        return true;
    };
    for index in 0..queue.length() {
        let Some(value) = queue.get_index(scope, index) else {
            continue;
        };
        let Ok(candidate) = v8::Local::<v8::Object>::try_from(value) else {
            continue;
        };
        if candidate.strict_equals(transaction.into()) {
            return true;
        }
        if transaction_db_key(scope, candidate).as_deref() == Some(&db_key)
            && !object_bool_property(scope, candidate, INDEXED_DB_TRANSACTION_FINISHED_SLOT)
                .unwrap_or(false)
        {
            return false;
        }
    }
    true
}

pub(in crate::context_bootstrap::indexed_db) fn has_unfinished_readwrite_transaction_for_db(
    scope: &mut v8::PinScope<'_, '_>,
    db_key: &str,
) -> bool {
    let Some(queue) = indexed_db_runtime_array(scope, IndexedDbRuntimeArray::ReadwriteTransactions)
    else {
        return false;
    };
    for index in 0..queue.length() {
        let Some(value) = queue.get_index(scope, index) else {
            continue;
        };
        let Ok(transaction) = v8::Local::<v8::Object>::try_from(value) else {
            continue;
        };
        if transaction_db_key(scope, transaction).as_deref() == Some(db_key)
            && !object_bool_property(scope, transaction, INDEXED_DB_TRANSACTION_FINISHED_SLOT)
                .unwrap_or(false)
        {
            return true;
        }
    }
    false
}
