use super::*;

pub(in crate::context_bootstrap) fn font_face_set_add_event_listener_callback<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue<'s, v8::Value>,
) {
    simple_object_event_target_add_listener(scope, &args, FONT_FACE_SET_LISTENERS_SLOT);
    rv.set_undefined();
}

pub(in crate::context_bootstrap) fn font_face_set_remove_event_listener_callback<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue<'s, v8::Value>,
) {
    simple_object_event_target_remove_listener(scope, &args, FONT_FACE_SET_LISTENERS_SLOT);
    rv.set_undefined();
}

pub(in crate::context_bootstrap) fn font_face_set_dispatch_event_callback<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    mut rv: v8::ReturnValue<'s, v8::Value>,
) {
    let event_value = args.get(0);
    if !event_value.is_object() || event_value.is_function() {
        rv.set(v8::Boolean::new(scope, false).into());
        return;
    }
    let Ok(event) = v8::Local::<v8::Object>::try_from(event_value) else {
        rv.set(v8::Boolean::new(scope, false).into());
        return;
    };
    let Some(event_type) = event
        .get(scope, v8str(scope, "type").into())
        .and_then(|value| value.to_string(scope))
        .map(|value| value.to_rust_string_lossy(scope))
    else {
        rv.set(v8::Boolean::new(scope, false).into());
        return;
    };
    let dispatched = dispatch_simple_event_target_event(
        scope,
        args.this(),
        FONT_FACE_SET_LISTENERS_SLOT,
        &event_type,
        event,
    );
    rv.set(v8::Boolean::new(scope, dispatched).into());
}
