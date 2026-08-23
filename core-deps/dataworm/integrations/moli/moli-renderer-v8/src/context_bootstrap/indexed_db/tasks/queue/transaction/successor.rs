use super::start::enqueue_transaction_start_task;
use super::*;

pub(in crate::context_bootstrap::indexed_db) fn enqueue_next_readwrite_transaction_start(
    scope: &mut v8::PinScope<'_, '_>,
    db_key: &str,
) {
    let Some(queue) = indexed_db_runtime_array(scope, IndexedDbRuntimeArray::ReadwriteTransactions)
    else {
        return;
    };
    for index in 0..queue.length() {
        let Some(value) = queue.get_index(scope, index) else {
            continue;
        };
        let Ok(transaction) = v8::Local::<v8::Object>::try_from(value) else {
            continue;
        };
        if transaction_db_key(scope, transaction).as_deref() != Some(db_key) {
            continue;
        }
        if object_bool_property(scope, transaction, INDEXED_DB_TRANSACTION_FINISHED_SLOT)
            .unwrap_or(false)
            || object_bool_property(scope, transaction, INDEXED_DB_TRANSACTION_STARTED_SLOT)
                .unwrap_or(false)
        {
            continue;
        }
        enqueue_transaction_start_task(scope, transaction);
        break;
    }
}
