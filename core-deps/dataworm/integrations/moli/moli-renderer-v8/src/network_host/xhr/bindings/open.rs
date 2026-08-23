use super::*;
use crate::webidl;

#[derive(webidl::WebIdlArgs)]
#[webidl(prefix = "XMLHttpRequest.open")]
struct XhrOpenArgs {
    #[webidl(required, converter = "byte_string")]
    method: String,
    #[webidl(required, converter = "usv_string")]
    url: String,
    #[webidl(default = true)]
    async_request: bool,
    #[webidl(converter = "usv_string", nullable)]
    username: Option<String>,
    #[webidl(converter = "usv_string", nullable)]
    password: Option<String>,
}

pub(super) fn xhr_open_callback<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    _rv: v8::ReturnValue<'_, v8::Value>,
) {
    let xhr = args.this();
    let Some(parsed) = webidl::parse_args::<XhrOpenArgs>(scope, &args) else {
        return;
    };
    let method = match normalize_request_method(&parsed.method) {
        Ok(method) => method,
        Err(message) => {
            throw_type_error(scope, message);
            return;
        }
    };
    // Keep username/password WebIDL conversion side effects even though request
    // auth plumbing is not modeled by the current XHR transport state yet.
    let _ = (&parsed.username, &parsed.password);
    let timeout = xhr_state_number_property(scope, xhr, XHR_TIMEOUT_SLOT).unwrap_or(0.0);
    if timeout != 0.0 && xhr_is_synchronous_document_request(scope, parsed.async_request) {
        xhr_throw_invalid_access(
            scope,
            "Failed to execute 'open' on 'XMLHttpRequest': Synchronous requests must not set a timeout.",
        );
        return;
    }
    let response_type = xhr_state_string_property(scope, xhr, XHR_RESPONSE_TYPE_SLOT)
        .as_deref()
        .and_then(XmlHttpRequestResponseType::parse)
        .unwrap_or(XmlHttpRequestResponseType::Default);
    if response_type != XmlHttpRequestResponseType::Default
        && xhr_is_synchronous_document_request(scope, parsed.async_request)
    {
        xhr_throw_invalid_access(
            scope,
            "Failed to execute 'open' on 'XMLHttpRequest': Synchronous requests from a document must not set a response type.",
        );
        return;
    }
    let previous_ready_state =
        xhr_state_number_property(scope, xhr, XHR_READY_STATE_SLOT).unwrap_or(0.0) as u32;
    super::super::delivery::cancel_xhr_timeout(scope, xhr);
    super::super::delivery::clear_xhr_progress_throttle(scope, xhr);
    super::super::delivery::clear_xhr_timeout_start(scope, xhr);
    let open_generation =
        xhr_state_number_property(scope, xhr, XHR_OPEN_GENERATION_SLOT).unwrap_or(0.0);
    set_xhr_state_number(scope, xhr, XHR_OPEN_GENERATION_SLOT, open_generation + 1.0);
    set_xhr_state_string(scope, xhr, XHR_METHOD_SLOT, &method);
    set_xhr_state_string(scope, xhr, XHR_URL_SLOT, &parsed.url);
    set_xhr_state_string(scope, xhr, XHR_REQUEST_HEADERS_SLOT, "[]");
    set_xhr_state_bool(scope, xhr, XHR_ASYNC_SLOT, parsed.async_request);
    set_xhr_state_number(scope, xhr, XHR_READY_STATE_SLOT, 1.0);
    set_xhr_state_number(scope, xhr, XHR_STATUS_SLOT, 0.0);
    set_xhr_state_string(scope, xhr, XHR_STATUS_TEXT_SLOT, "");
    set_xhr_state_string(scope, xhr, XHR_RESPONSE_TEXT_SLOT, "");
    set_xhr_state_string(scope, xhr, XHR_RESPONSE_URL_SLOT, "");
    set_xhr_state_string(scope, xhr, XHR_RESPONSE_HEADERS_SLOT, "[]");
    set_xhr_state_string(scope, xhr, XHR_PENDING_KIND_SLOT, "");
    set_xhr_state_string(scope, xhr, XHR_PENDING_URL_SLOT, "");
    set_xhr_state_string(scope, xhr, XHR_PENDING_BODY_SLOT, "");
    set_xhr_state_value(
        scope,
        xhr,
        XHR_PENDING_BODY_BYTES_SLOT,
        v8::undefined(scope).into(),
    );
    set_xhr_state_string(scope, xhr, XHR_PENDING_HEADERS_SLOT, "[]");
    set_xhr_state_number(scope, xhr, XHR_ACTIVE_INTERNAL_ID_SLOT, 0.0);
    set_xhr_state_number(scope, xhr, XHR_PENDING_STATUS_SLOT, 0.0);
    set_xhr_state_bool(scope, xhr, XHR_ABORTED_SLOT, false);
    set_xhr_state_bool(scope, xhr, XHR_SEND_FLAG_SLOT, false);
    let empty_response: v8::Local<'_, v8::Value> = v8_string(scope, "")
        .map(|s| s.into())
        .unwrap_or_else(|| v8::undefined(scope).into());
    set_xhr_state_value(scope, xhr, XHR_RESPONSE_SLOT, empty_response);
    set_xhr_state_value(scope, xhr, XHR_RESPONSE_XML_SLOT, v8::null(scope).into());
    if previous_ready_state != 1 {
        xhr_fire_readystatechange(scope, xhr, 1);
    }
}
