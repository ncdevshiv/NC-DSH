use super::*;

pub(super) fn try_dispatch_index_read_operation<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    operation: &operation::QueuedTransactionOperation<'s>,
) -> bool {
    match &operation.kind {
        IndexedDbTransactionOperationKindLocals::IndexGet { query } => {
            execute_index_get_request(
                scope,
                operation.source,
                operation.request,
                operation.handle,
                &operation.store_name,
                *query,
            );
        }
        IndexedDbTransactionOperationKindLocals::IndexGetKey { query } => {
            execute_index_get_key_request(
                scope,
                operation.source,
                operation.request,
                operation.handle,
                &operation.store_name,
                *query,
            );
        }
        IndexedDbTransactionOperationKindLocals::IndexGetAll {
            query,
            count,
            direction,
        } => {
            let direction = operation::collection_direction_from_value(scope, *direction);
            execute_index_get_all_request(
                scope,
                operation.source,
                operation.request,
                operation.handle,
                &operation.store_name,
                *query,
                *count,
                direction,
            );
        }
        IndexedDbTransactionOperationKindLocals::IndexGetAllKeys {
            query,
            count,
            direction,
        } => {
            let direction = operation::collection_direction_from_value(scope, *direction);
            execute_index_get_all_keys_request(
                scope,
                operation.source,
                operation.request,
                operation.handle,
                &operation.store_name,
                *query,
                *count,
                direction,
            );
        }
        IndexedDbTransactionOperationKindLocals::IndexCount { query } => {
            execute_index_count_request(
                scope,
                operation.source,
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
