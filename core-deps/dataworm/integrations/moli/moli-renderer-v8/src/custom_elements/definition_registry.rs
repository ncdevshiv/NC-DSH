use super::super::util::v8_string;
use super::definition::{CustomElementStore, PendingWhenDefined};
use super::definition_builder::{
    build_custom_element_definition, constructor_source_is_non_constructable,
    is_supported_built_in_extends_target,
};
use super::definition_error::CustomElementDefineError;
use crate::dom::custom_elements::is_valid_custom_element_name;

impl CustomElementStore {
    pub(crate) fn define<'s>(
        &mut self,
        scope: &mut v8::PinScope<'s, '_>,
        name: &str,
        constructor: v8::Local<'s, v8::Function>,
        extends_local_name: Option<String>,
    ) -> Result<bool, CustomElementDefineError> {
        let constructor_value: v8::Local<'_, v8::Value> = constructor.into();
        if constructor_value.is_async_function() || constructor_value.is_generator_function() {
            return Err(CustomElementDefineError::ConstructorNotConstructable);
        }
        if constructor_source_is_non_constructable(scope, constructor_value)? {
            return Err(CustomElementDefineError::ConstructorNotConstructable);
        }
        if !is_valid_custom_element_name(name) {
            return Err(CustomElementDefineError::InvalidName(name.to_owned()));
        }
        if self.definitions.contains_key(name) {
            return Err(CustomElementDefineError::NameAlreadyDefined(
                name.to_owned(),
            ));
        }
        if self.definitions.values().any(|definition| {
            let registered_constructor = v8::Local::new(scope, &definition.constructor);
            registered_constructor.strict_equals(constructor.into())
        }) {
            return Err(CustomElementDefineError::ConstructorAlreadyRegistered);
        }
        if let Some(extends_local_name) = extends_local_name.as_deref()
            && !is_supported_built_in_extends_target(extends_local_name)
        {
            return Err(CustomElementDefineError::InvalidExtendsTarget(
                extends_local_name.to_owned(),
            ));
        }
        if self.definition_is_running {
            return Err(CustomElementDefineError::DefinitionAlreadyRunning);
        }

        self.definition_is_running = true;
        let definition_result =
            build_custom_element_definition(scope, constructor, extends_local_name);
        self.definition_is_running = false;
        let definition = definition_result?;
        let disables_shadow = definition.disables_shadow;
        self.definitions.insert(name.to_owned(), definition);
        self.resolve_when_defined(scope, name);
        Ok(disables_shadow)
    }

    pub(crate) fn definition_constructor<'s>(
        &self,
        scope: &mut v8::PinScope<'s, '_>,
        name: &str,
    ) -> Option<v8::Local<'s, v8::Function>> {
        self.definitions
            .get(name)
            .map(|definition| v8::Local::new(scope, &definition.constructor))
    }

    pub(crate) fn get<'s>(
        &self,
        scope: &mut v8::PinScope<'s, '_>,
        name: &str,
    ) -> Option<v8::Local<'s, v8::Function>> {
        self.definition_constructor(scope, name)
    }

    pub(crate) fn get_name<'s>(
        &self,
        scope: &mut v8::PinScope<'s, '_>,
        constructor: v8::Local<'_, v8::Function>,
    ) -> Option<v8::Local<'s, v8::String>> {
        let name = self.definition_name_for_constructor(scope, constructor)?;
        v8_string(scope, &name)
    }

    pub(crate) fn definition_name_for_constructor(
        &self,
        scope: &mut v8::PinScope<'_, '_>,
        constructor: v8::Local<'_, v8::Function>,
    ) -> Option<String> {
        self.definition_for_constructor(scope, constructor)
            .map(|(name, _)| name)
    }

    pub(crate) fn definition_for_constructor(
        &self,
        scope: &mut v8::PinScope<'_, '_>,
        constructor: v8::Local<'_, v8::Function>,
    ) -> Option<(String, Option<String>)> {
        self.definitions
            .iter()
            .find(|(_, definition)| {
                let registered_constructor = v8::Local::new(scope, &definition.constructor);
                registered_constructor.strict_equals(constructor.into())
            })
            .map(|(name, definition)| (name.clone(), definition.extends_local_name.clone()))
    }

    pub(crate) fn when_defined<'s>(
        &mut self,
        scope: &mut v8::PinScope<'s, '_>,
        name: &str,
    ) -> Option<v8::Local<'s, v8::Promise>> {
        if let Some(constructor) = self.definition_constructor(scope, name) {
            let resolver = v8::PromiseResolver::new(scope)?;
            let _ = resolver.resolve(scope, constructor.into());
            return Some(resolver.get_promise(scope));
        }

        if let Some(pending) = self.pending_when_defined.get(name) {
            return Some(v8::Local::new(scope, &pending.promise));
        }

        let resolver = v8::PromiseResolver::new(scope)?;
        let promise = resolver.get_promise(scope);
        self.pending_when_defined.insert(
            name.to_owned(),
            PendingWhenDefined {
                promise: v8::Global::new(scope, promise),
                resolver: v8::Global::new(scope, resolver),
            },
        );
        Some(promise)
    }

    fn resolve_when_defined(&mut self, scope: &mut v8::PinScope<'_, '_>, name: &str) {
        let Some(pending) = self.pending_when_defined.remove(name) else {
            return;
        };
        let Some(constructor) = self.definition_constructor(scope, name) else {
            return;
        };
        let resolver = v8::Local::new(scope, &pending.resolver);
        let _ = resolver.resolve(scope, constructor.into());
    }
}
