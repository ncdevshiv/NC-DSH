use super::*;
use crate::{util::context_host_ptr_from_window_object, webidl};

pub(in crate::context_bootstrap) fn ensure_indexed_db_runtime_state<'s>(
    scope: &mut v8::PinScope<'s, '_>,
) -> Option<v8::Local<'s, v8::Object>> {
    indexed_db_runtime_factory(scope)
}

pub(crate) fn install_worker_indexed_db_runtime_state<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    global: v8::Local<'s, v8::Object>,
) -> Result<()> {
    global
        .set_lazy_data_property_with_configuration(
            scope,
            v8str(scope, "indexedDB").into(),
            v8::LazyDataPropertyConfiguration::new(worker_indexed_db_lazy_getter)
                .property_attribute(v8::PropertyAttribute::DONT_ENUM)
                .getter_side_effect_type(v8::SideEffectType::HasNoSideEffect),
        )
        .unwrap_or(false)
        .then_some(())
        .ok_or_else(|| anyhow!("failed to install lazy worker IndexedDB factory"))
}

fn worker_indexed_db_lazy_getter<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    _name: v8::Local<'s, v8::Name>,
    _args: v8::PropertyCallbackArguments<'s>,
    mut rv: v8::ReturnValue<'_, v8::Value>,
) {
    match ensure_indexed_db_runtime_state(scope) {
        Some(factory) => rv.set(factory.into()),
        None => rv.set(v8::undefined(scope).into()),
    }
}

pub(crate) fn window_indexed_db_getter<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue<'_, v8::Value>,
) {
    if context_host_ptr_from_window_object(scope, args.this()).is_none() {
        webidl::throw_type_error(
            scope,
            "Window.indexedDB getter called on incompatible receiver.",
        );
        return;
    }
    match ensure_indexed_db_runtime_state(scope) {
        Some(factory) => rv.set(factory.into()),
        None => rv.set(v8::undefined(scope).into()),
    }
}
