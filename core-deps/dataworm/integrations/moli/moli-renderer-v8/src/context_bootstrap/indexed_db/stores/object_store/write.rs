use super::*;

mod delete_clear;
mod put_add;

pub(in crate::context_bootstrap::indexed_db) use self::delete_clear::{
    idb_object_store_clear_callback, idb_object_store_delete_callback,
};
pub(in crate::context_bootstrap::indexed_db) use self::put_add::{
    idb_object_store_add_callback, idb_object_store_put_callback,
};
