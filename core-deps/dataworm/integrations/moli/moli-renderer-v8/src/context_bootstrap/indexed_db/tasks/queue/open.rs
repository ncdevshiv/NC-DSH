use super::*;

pub(in crate::context_bootstrap::indexed_db) fn enqueue_open_task<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    request: v8::Local<'s, v8::Object>,
    database: v8::Local<'s, v8::Object>,
    transaction: v8::Local<'s, v8::Object>,
    old_version: u64,
    new_version: u64,
) {
    let task = v8::Object::new(scope);
    register_indexed_db_open_task(
        scope,
        task,
        request,
        database,
        transaction,
        old_version,
        new_version,
    );
    enqueue_indexed_db_task(scope, task);
}
