use super::*;

pub(super) fn try_dispatch_object_store_read_operation<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    operation: &operation::QueuedTransactionOperation<'s>,
) -> bool {
    match &operation.kind {
        IndexedDbTransactionOperationKindLocals::ObjectStoreGet { query } => {
            execute_object_store_get_request(
                scope,
                operation.request,
                operation.handle,
                &operation.store_name,
                *query,
            );
        }
        IndexedDbTransactionOperationKindLocals::ObjectStoreGetAll {
            query,
            count,
            direction,
        } => {
            let direction = operation::collection_direction_from_value(scope, *direction);
            execute_object_store_get_all_request(
                scope,
                operation.request,
                operation.handle,
                &operation.store_name,
                *query,
                *count,
                direction,
            );
        }
        IndexedDbTransactionOperationKindLocals::ObjectStoreGetKey { query } => {
            execute_object_store_get_key_request(
                scope,
                operation.request,
                operation.handle,
                &operation.store_name,
                *query,
            );
        }
        IndexedDbTransactionOperationKindLocals::ObjectStoreGetAllKeys {
            query,
            count,
            direction,
        } => {
            let direction = operation::collection_direction_from_value(scope, *direction);
            execute_object_store_get_all_keys_request(
                scope,
                operation.request,
                operation.handle,
                &operation.store_name,
                *query,
                *count,
                direction,
            );
        }
        IndexedDbTransactionOperationKindLocals::ObjectStoreCount { query } => {
            execute_object_store_count_request(
                scope,
                operation.request,
                operation.handle,
                &operation.store_name,
                *query,
            );
        }
        _ => return false,
    }
    true
}
