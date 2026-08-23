use super::*;

mod get;
mod get_key;

pub(in crate::context_bootstrap::indexed_db) use self::get::idb_index_get_callback;
pub(in crate::context_bootstrap::indexed_db) use self::get_key::idb_index_get_key_callback;
