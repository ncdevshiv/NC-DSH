use super::super::*;
use crate::util::call_script_visible_function;

pub(super) fn detach_foreign_node_from_parent<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    object: v8::Local<'s, v8::Object>,
) {
    let Some(parent) = object.get(scope, v8str(scope, "parentNode").into()) else {
        return;
    };
    if parent.is_null_or_undefined() {
        return;
    }
    let Ok(parent) = v8::Local::<v8::Object>::try_from(parent) else {
        return;
    };
    let Some(remove_child) = parent.get(scope, v8str(scope, "removeChild").into()) else {
        return;
    };
    let Ok(remove_child) = v8::Local::<v8::Function>::try_from(remove_child) else {
        return;
    };
    let _ = call_script_visible_function(
        scope,
        remove_child,
        parent.into(),
        &[object.into()],
        "foreign node parent.removeChild",
    );
}
