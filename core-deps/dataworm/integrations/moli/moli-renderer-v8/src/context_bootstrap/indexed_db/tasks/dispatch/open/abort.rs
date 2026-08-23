use super::*;

pub(super) fn finish_aborted_upgrade_open<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    request: v8::Local<'s, v8::Object>,
) {
    if let Some(database) = object_property_as_object(scope, request, "result") {
        set_indexed_db_slot_value(
            scope,
            database,
            INDEXED_DB_DATABASE_UPGRADE_TRANSACTION_SLOT,
            v8::null(scope).into(),
        );
        close_indexed_db_database_connection(scope, database);
    }
    let error = dom_exception_value(scope, "The upgrade transaction was aborted.", "AbortError");
    set_indexed_db_request_surface_value(
        scope,
        request,
        INDEXED_DB_REQUEST_ERROR_SLOT,
        "error",
        error,
    );
    let done = v8str(scope, "done").into();
    set_indexed_db_request_surface_value(
        scope,
        request,
        INDEXED_DB_REQUEST_READY_STATE_SLOT,
        "readyState",
        done,
    );
    let undefined = v8::undefined(scope).into();
    set_indexed_db_request_surface_value(
        scope,
        request,
        INDEXED_DB_REQUEST_RESULT_SLOT,
        "result",
        undefined,
    );
    let _ = dispatch_idb_named_event(scope, request, "error", |_, _| {});
    release_request_dispatch_refs(scope, request);
}
