use super::*;

mod delete_clear;
mod put_add;

pub(in crate::context_bootstrap::indexed_db) use self::delete_clear::{
    execute_object_store_clear_request, execute_object_store_delete_request,
};
pub(in crate::context_bootstrap::indexed_db) use self::put_add::execute_object_store_write_request;
