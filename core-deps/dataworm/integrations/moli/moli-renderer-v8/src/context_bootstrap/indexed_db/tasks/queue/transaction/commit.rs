use super::*;

pub(in crate::context_bootstrap::indexed_db) fn enqueue_transaction_commit_task<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    transaction: v8::Local<'s, v8::Object>,
) {
    if object_bool_property(
        scope,
        transaction,
        INDEXED_DB_TRANSACTION_COMMIT_SCHEDULED_SLOT,
    )
    .unwrap_or(false)
        || object_bool_property(scope, transaction, INDEXED_DB_TRANSACTION_FINISHED_SLOT)
            .unwrap_or(false)
    {
        return;
    }
    set_indexed_db_slot_value(
        scope,
        transaction,
        INDEXED_DB_TRANSACTION_COMMIT_SCHEDULED_SLOT,
        v8::Boolean::new(scope, true).into(),
    );
    let task = create_transaction_task(scope, "transaction-commit", transaction);
    enqueue_indexed_db_task(scope, task);
}
