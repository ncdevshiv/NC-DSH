use super::*;

pub(super) fn dispatch_blocked_once<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    request: v8::Local<'s, v8::Object>,
    old_version: u64,
    new_version: Option<u64>,
) {
    if object_bool_property(scope, request, INDEXED_DB_REQUEST_BLOCKED_DISPATCHED_SLOT)
        .unwrap_or(false)
    {
        return;
    }
    set_indexed_db_slot_value(
        scope,
        request,
        INDEXED_DB_REQUEST_BLOCKED_DISPATCHED_SLOT,
        v8::Boolean::new(scope, true).into(),
    );
    let _ = dispatch_version_change_event(scope, request, "blocked", old_version, new_version);
}
