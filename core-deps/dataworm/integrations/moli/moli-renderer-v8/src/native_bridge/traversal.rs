mod algorithms;
mod filters;
mod identity;
mod node_iterator;
mod state;
mod tree_walker;
mod wrappers;

pub(super) use filters::TraversalFilter;
pub(super) use state::{NodeIteratorSnapshot, TraversalStore, TreeWalkerSnapshot};
pub(crate) use wrappers::install_traversal_template_bindings;
pub(super) use wrappers::{
    build_node_iterator_wrapper, build_node_iterator_wrapper_template, build_tree_walker_wrapper,
    build_tree_walker_wrapper_template,
};

pub(in crate::native_bridge::traversal) use node_iterator::{
    node_iterator_detach_callback, node_iterator_filter_getter, node_iterator_next_node_callback,
    node_iterator_pointer_before_reference_node_getter, node_iterator_previous_node_callback,
    node_iterator_reference_node_getter, node_iterator_root_getter,
    node_iterator_what_to_show_getter,
};
pub(in crate::native_bridge::traversal) use tree_walker::{
    tree_walker_current_node_getter, tree_walker_current_node_setter, tree_walker_filter_getter,
    tree_walker_first_child_callback, tree_walker_last_child_callback,
    tree_walker_next_node_callback, tree_walker_next_sibling_callback,
    tree_walker_parent_node_callback, tree_walker_previous_node_callback,
    tree_walker_previous_sibling_callback, tree_walker_root_getter,
    tree_walker_what_to_show_getter,
};
