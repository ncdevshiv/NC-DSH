use super::*;

mod delete;
mod update;

pub(in crate::context_bootstrap::indexed_db) use self::delete::idb_cursor_delete_callback;
pub(in crate::context_bootstrap::indexed_db) use self::update::idb_cursor_update_callback;
