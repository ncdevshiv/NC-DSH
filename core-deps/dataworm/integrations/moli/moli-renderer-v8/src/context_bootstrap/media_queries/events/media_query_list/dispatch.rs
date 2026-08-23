use super::*;

pub(in crate::context_bootstrap) fn dispatch_media_query_list_event<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    target: v8::Local<'s, v8::Object>,
    event: v8::Local<'s, v8::Object>,
) -> bool {
    let Some(event_type) = object_string_property_defined(scope, event, "type") else {
        return false;
    };
    dispatch_simple_event_target_event(
        scope,
        target,
        MEDIA_QUERY_LIST_LISTENERS_SLOT,
        &event_type,
        event,
    )
}

pub(crate) fn media_query_list_dispatch_event_callback<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue<'s, v8::Value>,
) {
    let event_value = args.get(0);
    if !event_value.is_object() || event_value.is_function() {
        throw_type_error(
            scope,
            "Failed to execute 'dispatchEvent': parameter 1 is not an object.",
        );
        return;
    }
    let Ok(event) = v8::Local::<v8::Object>::try_from(event_value) else {
        throw_type_error(
            scope,
            "Failed to execute 'dispatchEvent': parameter 1 is not an object.",
        );
        return;
    };
    if object_string_property_defined(scope, event, "type").is_none() {
        throw_type_error(
            scope,
            "Failed to execute 'dispatchEvent': event type is required.",
        );
        return;
    }

    let default_allowed = dispatch_media_query_list_event(scope, args.this(), event);
    rv.set(v8::Boolean::new(scope, default_allowed).into());
}
