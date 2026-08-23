use super::*;

mod character_data;
mod document;
mod element;
mod element_form;
mod node;

pub(in crate::native_bridge::document) use character_data::{
    install_detached_character_data_instance_properties,
    install_detached_processing_instruction_instance_properties,
};
pub(in crate::native_bridge::document) use document::install_detached_document_instance_properties;
pub(in crate::native_bridge::document) use element::{
    install_detached_anchor_instance_properties, install_detached_element_instance_properties,
    install_detached_form_associated_instance_properties,
    install_detached_iframe_instance_properties, install_detached_image_instance_properties,
    install_detached_label_instance_properties, install_detached_option_instance_properties,
    install_detached_select_instance_properties,
    install_detached_text_replacement_instance_properties,
};
pub(in crate::native_bridge) use element_form::{
    detached_form_reset_callback, detached_form_submit_callback,
};
pub(in crate::native_bridge::document) use element_form::{
    install_detached_form_control_instance_properties, install_detached_form_instance_properties,
};
pub(in crate::native_bridge::document) use node::{
    install_detached_document_type_instance_properties,
    install_detached_node_core_instance_properties,
    install_detached_non_document_type_child_node_instance_properties,
    install_detached_parent_node_instance_properties,
};
