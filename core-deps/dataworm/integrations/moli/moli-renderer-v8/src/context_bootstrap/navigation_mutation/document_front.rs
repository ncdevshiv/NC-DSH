use super::*;

pub(in crate::context_bootstrap) fn sync_local_document_front_from_window<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    owner: v8::Local<'s, v8::Object>,
) {
    if runtime_window_is_global(scope, owner) {
        return;
    }
    let Some(document) = owner
        .get(scope, v8str(scope, "document").into())
        .and_then(|value| v8::Local::<v8::Object>::try_from(value).ok())
    else {
        return;
    };
    super::sync_document_location_runtime_state_from_window(scope, document, owner);
}
