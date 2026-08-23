use std::cell::RefCell;

use indexmap::IndexMap;
use style::{stylesheets::UrlExtraData, stylist::RegisterCustomPropertyResult};

use super::MoliStyleEngine;
use crate::dom::native::DomHost;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CssCustomPropertyRegistration {
    pub(crate) name: String,
    pub(crate) syntax: String,
    pub(crate) inherits: bool,
    pub(crate) initial_value: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CssCustomPropertyRegistrationError {
    AlreadyRegistered,
}

#[derive(Debug, Default)]
pub(super) struct CssCustomPropertyRegistry {
    registrations: RefCell<IndexMap<String, CssCustomPropertyRegistration>>,
}

impl CssCustomPropertyRegistry {
    pub(super) fn new() -> Self {
        Self::default()
    }

    pub(super) fn clear(&self) {
        self.registrations.borrow_mut().clear();
    }

    fn register(
        &self,
        registration: CssCustomPropertyRegistration,
        _base_url: url::Url,
    ) -> Result<(), CssCustomPropertyRegistrationError> {
        let mut registrations = self.registrations.borrow_mut();
        if registrations.contains_key(&registration.name) {
            return Err(CssCustomPropertyRegistrationError::AlreadyRegistered);
        }
        registrations.insert(registration.name.clone(), registration);
        Ok(())
    }

    fn registration(&self, name: &str) -> Option<CssCustomPropertyRegistration> {
        self.registrations.borrow().get(name).cloned()
    }

    pub(super) fn registrations(&self) -> Vec<CssCustomPropertyRegistration> {
        self.registrations.borrow().values().cloned().collect()
    }
}

impl MoliStyleEngine {
    pub(crate) fn validate_css_custom_property_registration(
        &self,
        registration: &CssCustomPropertyRegistration,
        base_url: url::Url,
    ) -> RegisterCustomPropertyResult {
        style::stylist::Stylist::validate_custom_property_registration(
            &UrlExtraData::from(base_url),
            &registration.name,
            &registration.syntax,
            registration.initial_value.as_deref(),
        )
    }

    pub(crate) fn register_css_custom_property_for_document_with_host(
        &mut self,
        host: &DomHost,
        document: crate::document_runtime::DomHandle,
        registration: CssCustomPropertyRegistration,
        base_url: url::Url,
    ) -> Result<(), CssCustomPropertyRegistrationError> {
        let world = self.world_for_document(document);
        world
            .registered_custom_properties
            .register(registration, base_url)?;
        self.invalidate_author_stylesheet_set_for_world_with_host(host, &world);
        Ok(())
    }

    pub(crate) fn registered_css_custom_property_registration_for_document(
        &self,
        document: crate::document_runtime::DomHandle,
        name: &str,
    ) -> Option<CssCustomPropertyRegistration> {
        self.world_for_document(document)
            .registered_custom_properties
            .registration(name)
    }

    pub(crate) fn script_css_custom_property_registrations_for_document(
        &self,
        document: crate::document_runtime::DomHandle,
    ) -> Vec<CssCustomPropertyRegistration> {
        self.world_for_document(document)
            .registered_custom_properties
            .registrations()
    }
}
