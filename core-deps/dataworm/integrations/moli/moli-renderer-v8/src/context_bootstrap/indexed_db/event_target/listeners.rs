use super::*;

pub(in crate::context_bootstrap::indexed_db) fn event_listener_array<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    target: v8::Local<'s, v8::Object>,
    event_type: &str,
    create: bool,
) -> Option<v8::Local<'s, v8::Array>> {
    let registry = if let Some(existing) =
        object_property_as_object(scope, target, INDEXED_DB_EVENT_LISTENERS_SLOT)
    {
        existing
    } else if create {
        // Event type is a page-controlled dictionary key; keep the registry
        // null-prototype and read it own-only below.
        let registry = new_null_prototype_object(scope);
        set_indexed_db_slot_value(
            scope,
            target,
            INDEXED_DB_EVENT_LISTENERS_SLOT,
            registry.into(),
        );
        registry
    } else {
        return None;
    };

    if let Some(existing) = object_own_property_as_array(scope, registry, event_type) {
        return Some(existing);
    }
    if !create {
        return None;
    }
    let listeners = v8::Array::new(scope, 0);
    set_indexed_db_internal_object_property(scope, registry, event_type, listeners.into());
    Some(listeners)
}

pub(in crate::context_bootstrap::indexed_db) fn event_listener_snapshot<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    target: v8::Local<'s, v8::Object>,
    event_type: &str,
) -> Vec<v8::Local<'s, v8::Function>> {
    let Some(listeners) = event_listener_array(scope, target, event_type, false) else {
        return Vec::new();
    };
    let mut snapshot = Vec::with_capacity(listeners.length() as usize);
    for index in 0..listeners.length() {
        let Some(value) = listeners.get_index(scope, index) else {
            continue;
        };
        let Ok(listener) = v8::Local::<v8::Function>::try_from(value) else {
            continue;
        };
        snapshot.push(listener);
    }
    snapshot
}
