use crate::dom::{
    custom_elements::is_valid_built_in_extends_name as is_valid_dom_built_in_extends_name,
    native::html_element_interface_name,
};

pub(super) fn is_supported_built_in_extends_target(name: &str) -> bool {
    is_valid_dom_built_in_extends_name(name)
        && html_element_interface_name(name) != "HTMLUnknownElement"
}
