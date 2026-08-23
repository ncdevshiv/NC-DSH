use super::*;

pub(in crate::context_bootstrap::indexed_db) fn idb_cursor_delete_callback<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue<'s, v8::Value>,
) {
    let cursor = args.this();
    let Some(position) = cursor_current_position(scope, cursor) else {
        let error = dom_exception_value(scope, "The cursor is exhausted.", "InvalidStateError");
        scope.throw_exception(error);
        return;
    };
    let Some((request, handle, store_name)) = create_cursor_request(scope, cursor) else {
        let error = dom_exception_value(
            scope,
            "The transaction is not active.",
            "TransactionInactiveError",
        );
        scope.throw_exception(error);
        return;
    };
    let Some(primary_key) = cursor_primary_key_at(scope, cursor, position) else {
        let error = dom_exception_value(scope, "The cursor is exhausted.", "InvalidStateError");
        scope.throw_exception(error);
        return;
    };
    match with_indexed_db_manager(scope, |manager| {
        manager.delete(handle, &store_name, &primary_key)
    }) {
        Ok(()) => store_request_success(scope, request, v8::undefined(scope).into()),
        Err(error) => {
            let error = request_error_object(scope, &error);
            store_request_error(scope, request, error);
        }
    }
    rv.set(request.into());
}
