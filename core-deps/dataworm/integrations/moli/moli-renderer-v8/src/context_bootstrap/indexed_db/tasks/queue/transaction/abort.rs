use super::*;

pub(in crate::context_bootstrap::indexed_db) fn enqueue_transaction_abort_task<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    transaction: v8::Local<'s, v8::Object>,
) {
    if object_bool_property(
        scope,
        transaction,
        INDEXED_DB_TRANSACTION_ABORT_DISPATCHED_SLOT,
    )
    .unwrap_or(false)
    {
        return;
    }
    set_indexed_db_slot_value(
        scope,
        transaction,
        INDEXED_DB_TRANSACTION_ABORT_DISPATCHED_SLOT,
        v8::Boolean::new(scope, true).into(),
    );
    let task = create_transaction_task(scope, "transaction-abort", transaction);
    enqueue_indexed_db_task(scope, task);
}
