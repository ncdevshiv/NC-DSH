use super::*;

pub(super) fn enqueue_cursor_result<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    cursor: v8::Local<'s, v8::Object>,
    next_position: Option<usize>,
) -> Option<()> {
    let (request, transaction) = cursor_request_and_transaction(scope, cursor)?;
    if !object_bool_property(scope, transaction, INDEXED_DB_TRANSACTION_ACTIVE_SLOT)
        .unwrap_or(false)
    {
        let error = dom_exception_value(
            scope,
            "The transaction is not active.",
            "TransactionInactiveError",
        );
        scope.throw_exception(error);
        return None;
    }
    queue_transaction_request(scope, transaction, request);
    prepare_cursor_request(scope, request);
    set_indexed_db_slot_value(
        scope,
        request,
        INDEXED_DB_PENDING_CURSOR_SLOT,
        cursor.into(),
    );
    let position = next_position
        .map(|position| position as f64)
        .unwrap_or(-1.0);
    set_indexed_db_slot_value(
        scope,
        request,
        INDEXED_DB_PENDING_CURSOR_POSITION_SLOT,
        v8::Number::new(scope, position).into(),
    );
    if next_position.is_some() {
        store_request_success(scope, request, cursor.into());
    } else {
        store_request_success(scope, request, v8::null(scope).into());
    }
    Some(())
}
