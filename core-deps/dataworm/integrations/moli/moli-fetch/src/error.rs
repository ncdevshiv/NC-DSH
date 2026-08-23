use anyhow::{Result, bail};
use http::StatusCode;

pub const NET_ERR_ABORTED_ERROR_TEXT: &str = "net::ERR_ABORTED";

pub(crate) fn browser_network_error_text(error: &anyhow::Error) -> &'static str {
    let error_chain = format!("{error:#}");
    if error_chain.contains("request cancelled") || error_chain.contains("Callback aborted") {
        return NET_ERR_ABORTED_ERROR_TEXT;
    }

    let Some(error) = error.downcast_ref::<curl::Error>() else {
        return "net::ERR_FAILED";
    };
    if error.is_couldnt_resolve_host() {
        "net::ERR_NAME_NOT_RESOLVED"
    } else if error.is_couldnt_resolve_proxy() {
        "net::ERR_PROXY_CONNECTION_FAILED"
    } else if error.is_couldnt_connect() {
        "net::ERR_CONNECTION_REFUSED"
    } else if error.is_operation_timedout() {
        "net::ERR_TIMED_OUT"
    } else if error.is_recv_error() {
        "net::ERR_CONNECTION_RESET"
    } else if error.is_got_nothing() {
        "net::ERR_EMPTY_RESPONSE"
    } else if error.is_too_many_redirects() {
        "net::ERR_TOO_MANY_REDIRECTS"
    } else if error.is_peer_failed_verification()
        || error.is_ssl_cacert()
        || error.is_ssl_certproblem()
    {
        "net::ERR_CERT_AUTHORITY_INVALID"
    } else {
        "net::ERR_FAILED"
    }
}

pub fn ensure_http_status_success(
    request_url: &str,
    status: u16,
    allow_http_auth_challenge_status: bool,
) -> Result<()> {
    if (200..=299).contains(&status) {
        return Ok(());
    }

    if allow_http_auth_challenge_status && matches!(status, 401 | 407) {
        return Ok(());
    }

    let reason = StatusCode::from_u16(status)
        .ok()
        .and_then(|status| status.canonical_reason())
        .unwrap_or("Unknown");
    bail!("HTTP request `{request_url}` returned {} {reason}", status)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curl_receive_failure_maps_to_browser_connection_reset() {
        let error = anyhow::Error::new(curl::Error::new(curl_sys::CURLE_RECV_ERROR))
            .context("curl request failed");

        assert_eq!(
            browser_network_error_text(&error),
            "net::ERR_CONNECTION_RESET"
        );
    }
}
