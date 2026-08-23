use super::*;

mod add;
mod operation;
mod put;

pub(in crate::context_bootstrap::indexed_db) use self::add::idb_object_store_add_callback;
pub(in crate::context_bootstrap::indexed_db) use self::put::idb_object_store_put_callback;
