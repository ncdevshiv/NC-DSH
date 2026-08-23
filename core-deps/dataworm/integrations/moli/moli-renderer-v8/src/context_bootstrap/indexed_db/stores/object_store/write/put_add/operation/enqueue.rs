use super::*;

pub(super) fn enqueue_deferred_object_store_write<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    transaction: v8::Local<'s, v8::Object>,
    store: v8::Local<'s, v8::Object>,
    request: v8::Local<'s, v8::Object>,
    store_name: &str,
    value: v8::Local<'s, v8::Value>,
    key: v8::Local<'s, v8::Value>,
    add_only: bool,
) {
    enqueue_transaction_operation(
        scope,
        transaction,
        store,
        request,
        store_name,
        IndexedDbTransactionOperationInput::ObjectStoreWrite {
            value,
            key,
            add_only,
        },
    );
}
