use super::*;

mod fail;
mod run_waiting;
mod success;

pub(in crate::context_bootstrap::indexed_db) fn flush_transaction_start_task<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    task: v8::Local<'s, v8::Object>,
) {
    let Some(transaction) = indexed_db_transaction_task_transaction(scope, task) else {
        return;
    };
    set_indexed_db_slot_value(
        scope,
        transaction,
        INDEXED_DB_TRANSACTION_START_SCHEDULED_SLOT,
        v8::Boolean::new(scope, false).into(),
    );
    if object_bool_property(scope, transaction, INDEXED_DB_TRANSACTION_STARTED_SLOT)
        .unwrap_or(false)
        || object_bool_property(scope, transaction, INDEXED_DB_TRANSACTION_FINISHED_SLOT)
            .unwrap_or(false)
        || !readwrite_transaction_can_start(scope, transaction)
    {
        return;
    }
    let Some(database) = object_property_as_object(scope, transaction, "db") else {
        return;
    };
    let Some(handle) = database_handle_from_value(scope, database.into()) else {
        return;
    };
    let store_names = object_property_as_object(scope, transaction, "objectStoreNames")
        .map(|names| dom_string_list_values(scope, names))
        .unwrap_or_default();
    match with_indexed_db_manager(scope, |manager| {
        manager.begin_transaction(handle, &store_names, TransactionMode::ReadWrite)
    }) {
        Ok(transaction_handle) => {
            success::start_readwrite_transaction(scope, transaction, transaction_handle);
        }
        Err(error) => {
            fail::fail_readwrite_transaction_start(scope, transaction, &error);
        }
    }
}
