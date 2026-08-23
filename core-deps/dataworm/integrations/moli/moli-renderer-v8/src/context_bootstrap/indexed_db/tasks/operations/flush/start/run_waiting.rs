use super::*;

pub(super) fn run_operations_waiting_for_start<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    transaction: v8::Local<'s, v8::Object>,
) {
    for operation in take_indexed_db_operations_waiting_for_start(scope, transaction) {
        operation::flush_queued_transaction_operation(scope, transaction, operation);
    }
}
