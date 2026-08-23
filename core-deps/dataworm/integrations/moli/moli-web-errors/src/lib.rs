//! Shared DOMException names, legacy codes, and default messages.

mod descriptor;
mod registry;

pub use descriptor::{
    DomExceptionConstant, DomExceptionDescriptor, dom_exception_default_message,
    dom_exception_descriptor, dom_exception_legacy_code, is_dom_exception_name,
};
pub use registry::{DOM_EXCEPTION_CONSTANTS, DOM_EXCEPTION_DESCRIPTORS};

#[cfg(test)]
mod tests;
