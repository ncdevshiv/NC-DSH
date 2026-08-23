use crate::dom::native::Node;

use super::super::{callback_arg_dom_handle, collections::build_collection_wrapper};
use super::*;

mod lookup;
mod mutation;
mod relationships;
mod state;

pub(in crate::native_bridge) use mutation::{
    bridge_append_child_callback, bridge_insert_before_callback, bridge_remove_child_callback,
    bridge_set_text_content_callback,
};
pub(in crate::native_bridge) use relationships::{
    bridge_first_child_callback, bridge_last_child_callback, bridge_next_sibling_callback,
    bridge_owner_document_callback, bridge_parent_node_callback, bridge_previous_sibling_callback,
};
pub(in crate::native_bridge) use state::{
    bridge_child_nodes_callback, bridge_contains_callback, bridge_describe_node_callback,
    bridge_text_content_callback,
};
