use super::*;

pub(in crate::context_bootstrap::indexed_db) fn database_handle_from_value(
    scope: &mut v8::PinScope<'_, '_>,
    value: v8::Local<'_, v8::Value>,
) -> Option<DatabaseHandle> {
    let object = v8::Local::<v8::Object>::try_from(value).ok()?;
    let handle = object_number_property(scope, object, INDEXED_DB_DATABASE_HANDLE_SLOT)?;
    Some(DatabaseHandle::from_raw(handle as u64))
}

pub(in crate::context_bootstrap::indexed_db) fn transaction_handle_from_value(
    scope: &mut v8::PinScope<'_, '_>,
    value: v8::Local<'_, v8::Value>,
) -> Option<TransactionHandle> {
    let object = v8::Local::<v8::Object>::try_from(value).ok()?;
    let handle = object_number_property(scope, object, INDEXED_DB_TRANSACTION_HANDLE_SLOT)?;
    Some(TransactionHandle::from_raw(handle as u64))
}
