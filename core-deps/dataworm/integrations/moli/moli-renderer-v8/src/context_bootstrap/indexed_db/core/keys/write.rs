use super::*;

mod clone;
mod extraction;
mod injection;
mod prepare;

use self::clone::clone_js_value;
use self::injection::inject_key_path_into_value;

pub(in crate::context_bootstrap::indexed_db) use self::extraction::{
    derive_object_store_key_from_value, extract_index_keys_from_value,
};
pub(in crate::context_bootstrap::indexed_db) use self::prepare::prepare_object_store_write;
