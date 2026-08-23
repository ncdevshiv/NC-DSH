use super::*;
use crate::webidl;

#[derive(webidl::WebIdlArgs)]
#[webidl(prefix = "IDBEventTarget.addEventListener")]
struct IdbEventTargetAddEventListenerArgs<'s> {
    #[webidl(required, name = "type")]
    event_type: String,
    #[webidl(index = 1, converter = "raw")]
    listener: Option<v8::Local<'s, v8::Value>>,
}

#[derive(webidl::WebIdlArgs)]
#[webidl(prefix = "IDBEventTarget.removeEventListener")]
struct IdbEventTargetRemoveEventListenerArgs<'s> {
    #[webidl(required, name = "type")]
    event_type: String,
    #[webidl(index = 1, converter = "raw")]
    listener: Option<v8::Local<'s, v8::Value>>,
}

#[derive(webidl::WebIdlArgs)]
#[webidl(prefix = "IDBEventTarget.dispatchEvent")]
struct IdbEventTargetDispatchEventArgs<'s> {
    #[webidl(required, converter = "raw")]
    event: v8::Local<'s, v8::Value>,
}

pub(in crate::context_bootstrap::indexed_db) fn idb_event_target_add_event_listener_callback<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    _rv: v8::ReturnValue<'_, v8::Value>,
) {
    let Some(parsed) = webidl::parse_args::<IdbEventTargetAddEventListenerArgs<'s>>(scope, &args)
    else {
        return;
    };
    let Some(listener) = parsed.listener else {
        return;
    };
    let Ok(listener) = v8::Local::<v8::Function>::try_from(listener) else {
        return;
    };
    let target = args.this();
    let Some(listeners) = event_listener_array(scope, target, &parsed.event_type, true) else {
        return;
    };
    if !array_contains_strict(scope, listeners, listener.into()) {
        let _ = listeners.set_index(scope, listeners.length(), listener.into());
    }
}

pub(in crate::context_bootstrap::indexed_db) fn idb_event_target_remove_event_listener_callback<
    's,
>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    _rv: v8::ReturnValue<'_, v8::Value>,
) {
    let Some(parsed) =
        webidl::parse_args::<IdbEventTargetRemoveEventListenerArgs<'s>>(scope, &args)
    else {
        return;
    };
    let Some(listener) = parsed.listener else {
        return;
    };
    let Ok(listener) = v8::Local::<v8::Function>::try_from(listener) else {
        return;
    };
    let target = args.this();
    let Some(current) = event_listener_array(scope, target, &parsed.event_type, false) else {
        return;
    };
    let next = v8::Array::new(scope, 0);
    for index in 0..current.length() {
        let Some(candidate) = current.get_index(scope, index) else {
            continue;
        };
        if candidate.strict_equals(listener.into()) {
            continue;
        }
        let _ = next.set_index(scope, next.length(), candidate);
    }
    if let Some(registry) =
        object_property_as_object(scope, target, INDEXED_DB_EVENT_LISTENERS_SLOT)
    {
        set_indexed_db_internal_object_property(scope, registry, &parsed.event_type, next.into());
    }
}

pub(in crate::context_bootstrap::indexed_db) fn idb_event_target_dispatch_event_callback<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue<'s, v8::Value>,
) {
    let Some(parsed) = webidl::parse_args::<IdbEventTargetDispatchEventArgs<'s>>(scope, &args)
    else {
        return;
    };
    let value = parsed.event;
    if !value.is_object() || value.is_function() {
        throw_type_error(
            scope,
            "Failed to execute 'dispatchEvent': parameter 1 is not an object.",
        );
        return;
    }
    let Ok(event) = v8::Local::<v8::Object>::try_from(value) else {
        throw_type_error(
            scope,
            "Failed to execute 'dispatchEvent': parameter 1 is not an object.",
        );
        return;
    };
    let target = args.this();
    if indexed_db_typed_owner_scope(scope, target).is_none() {
        throw_type_error(scope, "Illegal invocation");
        return;
    }
    let dispatched = dispatch_idb_event_object(scope, target, event);
    rv.set(v8::Boolean::new(scope, dispatched).into());
}
