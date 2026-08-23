use crate::dom::native::Node;

use super::*;

mod content;
mod methods;
mod mutation_methods;
mod mutation_reactions;

pub(in crate::native_bridge) use content::set_text_content_in_reaction_scope;
pub(in crate::native_bridge) use methods::{
    node_compare_document_position_callback, node_contains_callback, node_get_root_node_callback,
    node_has_child_nodes_callback, node_is_default_namespace_callback, node_is_equal_node_callback,
    node_is_same_node_callback, node_lookup_namespace_uri_callback, node_lookup_prefix_callback,
};
pub(in crate::native_bridge) use mutation_methods::{
    node_clone_node_callback, node_normalize_callback,
};
pub(in crate::native_bridge) use mutation_reactions::{
    append_child_in_reaction_scope, append_child_to_current_reaction_queue,
    insert_before_in_reaction_scope, insert_before_to_current_reaction_queue,
    remove_child_in_reaction_scope, remove_child_to_current_reaction_queue,
};
