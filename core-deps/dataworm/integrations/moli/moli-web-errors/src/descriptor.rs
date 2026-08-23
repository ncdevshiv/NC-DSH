#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DomExceptionDescriptor {
    pub name: &'static str,
    pub legacy_code: u16,
    pub default_message: &'static str,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DomExceptionConstant {
    pub property: &'static str,
    pub value: u16,
}

use crate::DOM_EXCEPTION_DESCRIPTORS;

pub fn dom_exception_descriptor(name: &str) -> Option<&'static DomExceptionDescriptor> {
    DOM_EXCEPTION_DESCRIPTORS
        .iter()
        .find(|descriptor| descriptor.name == name)
}

pub fn dom_exception_legacy_code(name: &str) -> u16 {
    dom_exception_descriptor(name)
        .map(|descriptor| descriptor.legacy_code)
        .unwrap_or(0)
}

pub fn dom_exception_default_message(name: &str) -> Option<&'static str> {
    dom_exception_descriptor(name).map(|descriptor| descriptor.default_message)
}

pub fn is_dom_exception_name(name: &str) -> bool {
    dom_exception_descriptor(name).is_some()
}
