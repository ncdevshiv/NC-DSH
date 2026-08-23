use super::util::{v8_string, v8str};

pub(super) fn set_object_prototype(
    scope: &mut v8::PinScope<'_, '_>,
    object: v8::Local<'_, v8::Object>,
    constructor_name: &str,
) {
    let global = scope.get_current_context().global(scope);
    let Some(name) = v8_string(scope, constructor_name) else {
        return;
    };
    let Some(constructor) = global.get(scope, name.into()) else {
        return;
    };
    let Ok(constructor) = v8::Local::<v8::Object>::try_from(constructor) else {
        return;
    };
    let Some(prototype) = constructor.get(scope, v8str(scope, "prototype").into()) else {
        return;
    };
    let _ = object.set_prototype(scope, prototype);
}
