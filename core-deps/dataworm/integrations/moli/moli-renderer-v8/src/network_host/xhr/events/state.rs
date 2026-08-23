use super::super::*;

pub(in crate::network_host::xhr) fn xhr_is_aborted(
    scope: &mut v8::PinScope<'_, '_>,
    xhr: v8::Local<'_, v8::Object>,
) -> bool {
    xhr_state_bool_property(scope, xhr, XHR_ABORTED_SLOT).unwrap_or(false)
}

pub(in crate::network_host::xhr) fn xhr_is_async(
    scope: &mut v8::PinScope<'_, '_>,
    xhr: v8::Local<'_, v8::Object>,
) -> bool {
    xhr_state_bool_property(scope, xhr, XHR_ASYNC_SLOT).unwrap_or(true)
}
