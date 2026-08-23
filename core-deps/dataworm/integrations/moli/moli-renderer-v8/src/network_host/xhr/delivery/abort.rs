use super::super::events::xhr_dispatch_progress_event;
use super::super::*;

pub(crate) fn apply_xhr_abort(scope: &mut v8::PinScope<'_, '_>, xhr: v8::Local<'_, v8::Object>) {
    super::cancel_xhr_timeout(scope, xhr);
    super::clear_xhr_progress_throttle(scope, xhr);
    set_xhr_state_bool(scope, xhr, XHR_ABORTED_SLOT, true);
    set_xhr_state_bool(scope, xhr, XHR_SEND_FLAG_SLOT, false);
    set_xhr_state_number(scope, xhr, XHR_ACTIVE_INTERNAL_ID_SLOT, 0.0);
    set_xhr_state_number(scope, xhr, XHR_READY_STATE_SLOT, 0.0);
    super::reset_xhr_response_for_request_error(scope, xhr);
    super::super::send::dispatch_xhr_upload_abort_if_in_progress(scope, xhr);
    xhr_dispatch_progress_event(scope, xhr, "abort", 0.0, 0.0);
    xhr_dispatch_progress_event(scope, xhr, "loadend", 0.0, 0.0);
}
