use super::*;

mod character_data;
mod creation;
mod document_state;
mod element_attributes;
mod element_metadata;
mod mutation;
mod node_metadata;
mod tree;

pub(in crate::native_bridge) use self::character_data::{
    bridge_detached_character_data_getter_callback, bridge_detached_character_data_setter_callback,
};
pub(in crate::native_bridge) use self::character_data::{
    detached_character_data_append_data_callback, detached_character_data_delete_data_callback,
    detached_character_data_insert_data_callback, detached_character_data_length,
    detached_character_data_replace_data_callback, detached_character_data_substring_data_callback,
    detached_character_data_value, detached_text_split_text_callback,
    detached_text_whole_text_value, set_detached_character_data_value,
};
pub(in crate::native_bridge) use self::creation::*;
pub(in crate::native_bridge) use self::document_state::*;
pub(in crate::native_bridge) use self::element_attributes::*;
pub(in crate::native_bridge) use self::element_metadata::*;
pub(in crate::native_bridge) use self::mutation::*;
pub(in crate::native_bridge) use self::node_metadata::*;
pub(in crate::native_bridge) use self::tree::*;
