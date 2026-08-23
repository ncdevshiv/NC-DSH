use super::*;

pub(super) fn xhr_abort_callback(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments<'_>,
    _rv: v8::ReturnValue<'_, v8::Value>,
) {
    if crate::worker::try_worker_xhr_abort_callback(scope, &args) {
        return;
    }

    let xhr = args.this();
    let ready_state = xhr
        .get(scope, v8str(scope, "readyState").into())
        .and_then(|v| v.number_value(scope))
        .unwrap_or(0.0) as u32;

    if ready_state == 0 || ready_state == 4 {
        return;
    }

    super::super::delivery::cancel_xhr_timeout(scope, xhr);
    super::super::delivery::clear_xhr_progress_throttle(scope, xhr);
    super::super::delivery::clear_xhr_timeout_start(scope, xhr);
    let internal_id =
        xhr_state_number_property(scope, xhr, XHR_ACTIVE_INTERNAL_ID_SLOT).unwrap_or(0.0) as u64;
    if internal_id != 0
        && let Some(host_ptr) = context_host_ptr_from_global_bridge(scope)
    {
        let _ = unsafe { &mut *host_ptr }.abort_subresource_fetch(internal_id);
    }

    set_xhr_state_bool(scope, xhr, XHR_ABORTED_SLOT, true);
    set_xhr_state_bool(scope, xhr, XHR_SEND_FLAG_SLOT, false);
    set_xhr_state_number(scope, xhr, XHR_ACTIVE_INTERNAL_ID_SLOT, 0.0);
    set_xhr_state_number(scope, xhr, XHR_READY_STATE_SLOT, 4.0);
    super::super::delivery::reset_xhr_response_for_request_error(scope, xhr);
    dispatch_xhr_upload_abort_if_in_progress(scope, xhr);
    xhr_dispatch_progress_event(scope, xhr, "abort", 0.0, 0.0);
    xhr_dispatch_progress_event(scope, xhr, "loadend", 0.0, 0.0);
    set_xhr_state_number(scope, xhr, XHR_READY_STATE_SLOT, 0.0);
}
