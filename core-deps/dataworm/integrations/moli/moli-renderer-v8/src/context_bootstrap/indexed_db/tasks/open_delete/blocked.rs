use super::*;

mod delete_task;
mod drain;
mod event;
mod open_task;

pub(super) fn blocked_task_storage_scope<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    task: v8::Local<'s, v8::Object>,
) -> Option<IndexedDbStorageScope> {
    indexed_db_typed_task_storage_scope(scope, task)
}

pub(in crate::context_bootstrap::indexed_db) use self::delete_task::flush_delete_blocked_task;
pub(in crate::context_bootstrap::indexed_db) use self::drain::flush_drain_blocked_open_requests_task;
pub(in crate::context_bootstrap::indexed_db) use self::open_task::flush_open_blocked_task;
