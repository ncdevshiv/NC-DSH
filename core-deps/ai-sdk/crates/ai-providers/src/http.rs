//! Shared HTTP layer for provider adapters: request execution with typed
//! error mapping (auth, rate limit, provider, network, timeout) and
//! streaming bodies.
//!
//! All provider adapters default to [`HttpClient::shared()`], a process-wide
//! connection pool. Clones share the same underlying `reqwest` client, so
//! parallel requests to the same host multiplex over a single set of
//! kept-alive TCP/HTTP2 connections instead of each adapter opening its own
//! pool (which is what the official SDKs do — one client per client
//! instance).

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use ai_errors::{
    AiError, AuthenticationError, NetworkError, ProviderError, RateLimitError, SerializationError,
    TimeoutError,
};

/// A shared, connection-pooled HTTP client.
///
/// The inner [`reqwest::Client`] lives behind an [`Arc`], so cloning an
/// [`HttpClient`] is free and every clone shares one connection pool and one
/// request counter.
#[derive(Debug, Clone)]
pub struct HttpClient {
    inner: Arc<reqwest::Client>,
    requests: Arc<AtomicU64>,
    in_flight: Arc<AtomicU64>,
}

/// The process-wide pool, built once with long-lived, multiplexed connection
/// settings.
static SHARED: OnceLock<HttpClient> = OnceLock::new();

impl HttpClient {
    /// The process-wide shared pool, tuned for throughput: 32 idle
    /// connections per host, TCP keepalive, and HTTP/2 keepalive pings while
    /// idle so pooled connections stay warm across bursts.
    pub fn shared() -> HttpClient {
        SHARED
            .get_or_init(|| {
                Self::build(|builder| {
                    builder
                        .pool_max_idle_per_host(32)
                        .pool_idle_timeout(Duration::from_secs(120))
                        .tcp_keepalive(Duration::from_secs(60))
                        .http2_keep_alive_interval(Duration::from_secs(30))
                        .http2_keep_alive_timeout(Duration::from_secs(10))
                        .http2_keep_alive_while_idle(true)
                })
                .expect("building the shared http client cannot fail")
            })
            .clone()
    }

    /// A private, isolated client with its own pool. Use for tests or when
    /// an application wants a dedicated connection pool for a specific
    /// provider.
    pub fn new() -> Result<Self, AiError> {
        Self::build(|builder| builder)
    }

    /// Builds the client with a per-request connect timeout.
    pub fn new_with_connect_timeout(timeout: Duration) -> Result<Self, AiError> {
        Self::build(|builder| builder.connect_timeout(timeout))
    }

    fn build(
        tune: impl FnOnce(reqwest::ClientBuilder) -> reqwest::ClientBuilder,
    ) -> Result<Self, AiError> {
        let builder = reqwest::Client::builder()
            .user_agent(concat!("ai-sdk/", env!("CARGO_PKG_VERSION")))
            .pool_idle_timeout(Duration::from_secs(120));
        let inner = tune(builder).build().map_err(|e| {
            AiError::Internal(ai_errors::InternalError::with_source(
                "http client build failed",
                e,
            ))
        })?;
        Ok(Self {
            inner: Arc::new(inner),
            requests: Arc::new(AtomicU64::new(0)),
            in_flight: Arc::new(AtomicU64::new(0)),
        })
    }

    pub fn inner(&self) -> &reqwest::Client {
        &self.inner
    }

    /// True when this client shares its pool with `other` (they wrap the
    /// same underlying `reqwest` client).
    pub fn shares_pool_with(&self, other: &HttpClient) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }

    /// Starts building a POST request against this pool.
    pub fn post(&self, url: String) -> reqwest::RequestBuilder {
        self.inner.post(url)
    }

    /// Starts building a GET request against this pool.
    pub fn get(&self, url: String) -> reqwest::RequestBuilder {
        self.inner.get(url)
    }

    /// Executes a request through this pool, counting it for observability.
    ///
    /// The `requests` counter tracks every call and `in_flight` tracks
    /// concurrent executions, giving callers cheap insight into pool usage
    /// (useful for load monitoring and benchmarks).
    pub async fn execute(
        &self,
        builder: reqwest::RequestBuilder,
    ) -> Result<reqwest::Response, reqwest::Error> {
        self.requests.fetch_add(1, Ordering::Relaxed);
        self.in_flight.fetch_add(1, Ordering::Relaxed);
        let result = builder.send().await;
        self.in_flight.fetch_sub(1, Ordering::Relaxed);
        result
    }

    /// Total requests executed through this pool since creation.
    pub fn request_count(&self) -> u64 {
        self.requests.load(Ordering::Relaxed)
    }

    /// Requests currently in flight through this pool.
    pub fn in_flight(&self) -> u64 {
        self.in_flight.load(Ordering::Relaxed)
    }
}

/// Parses an OpenAI-style error body (`{"error": {"message", "type", "code"}}`)
/// or falls back to the raw text.
fn parse_error_body(text: &str) -> (String, Option<String>) {
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(text) {
        if let Some(error) = value.get("error") {
            let message = error
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or(text)
                .to_string();
            let code = error
                .get("code")
                .and_then(|c| c.as_str())
                .map(|c| c.to_string());
            return (message, code);
        }
    }
    (text.to_string(), None)
}

/// Maps an HTTP response to a typed [`AiError`] based on status code.
///
/// `retry_after` is the duration parsed from the response's `Retry-After`
/// header (see [`retry_after_from_headers`]); it takes precedence over any
/// provider-specific hint embedded in the error body.
pub async fn map_response_error(
    provider: &str,
    status: reqwest::StatusCode,
    retry_after: Option<Duration>,
    body: &[u8],
) -> AiError {
    let text = String::from_utf8_lossy(body);
    let (message, code) = parse_error_body(&text);

    match status.as_u16() {
        401 | 403 => AiError::Authentication(AuthenticationError::new(format!(
            "{provider} rejected the API key (HTTP {status}): {message}"
        ))),
        429 => {
            let mut err = RateLimitError::new(provider, message);
            err.retry_after = retry_after.or_else(|| parse_retry_after(&text));
            AiError::RateLimit(err)
        }
        400 | 404 | 405 | 415 | 422 => AiError::Provider(
            ProviderError::new(provider, message)
                .with_status(status.as_u16())
                .with_code(code.unwrap_or_default()),
        ),
        500..=599 => AiError::Provider(
            ProviderError::new(provider, message)
                .with_status(status.as_u16())
                .with_code(code.unwrap_or_default()),
        ),
        _ => AiError::Provider(
            ProviderError::new(provider, format!("unexpected HTTP {status}: {message}"))
                .with_status(status.as_u16()),
        ),
    }
}

/// Parses a single `Retry-After` value: the standard delta-seconds form
/// (integer seconds) yields a duration, fractional seconds are tolerated,
/// and the HTTP-date form is ignored (`None` — it cannot be resolved to a
/// duration without a clock reference here).
fn parse_retry_after_value(value: &str) -> Option<Duration> {
    let trimmed = value.trim();
    if let Ok(secs) = trimmed.parse::<u64>() {
        return Some(Duration::from_secs(secs));
    }
    match trimmed.parse::<f64>() {
        Ok(secs) if secs.is_finite() && secs >= 0.0 => Some(Duration::from_secs_f64(secs)),
        _ => None,
    }
}

/// Extracts the standard `Retry-After` response header as a duration.
///
/// Header hints take precedence over any `retry_after` field embedded in
/// an error body (see [`map_response_error`]).
pub fn retry_after_from_headers(headers: &reqwest::header::HeaderMap) -> Option<Duration> {
    headers
        .get("retry-after")
        .and_then(|v| v.to_str().ok())
        .and_then(parse_retry_after_value)
}

/// Extracts `Retry-After` from an OpenAI-style rate-limit payload when the
/// response carried no usable header hint.
fn parse_retry_after(body: &str) -> Option<Duration> {
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(body) {
        if let Some(after) = value
            .pointer("/error/retry_after")
            .or_else(|| value.get("retry_after"))
        {
            if let Some(secs) = after.as_f64() {
                return Some(Duration::from_secs_f64(secs.max(0.0)));
            }
        }
    }
    None
}

/// Maps a `reqwest` failure to a typed [`AiError`].
pub fn map_reqwest_error(operation: &str, err: reqwest::Error) -> AiError {
    if err.is_timeout() {
        AiError::Timeout(TimeoutError::new(operation, Duration::from_secs(0)))
    } else {
        AiError::Network(NetworkError::new(operation, err.to_string()))
    }
}

/// Parses a JSON body, mapping failures to a serialization error.
pub fn parse_json<T: serde::de::DeserializeOwned>(
    operation: &str,
    body: &[u8],
) -> Result<T, AiError> {
    serde_json::from_slice(body).map_err(|e| {
        AiError::Serialization(SerializationError::new(format!(
            "failed to parse {operation} response: {e}"
        )))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// Spawns a minimal HTTP/1.1 server that answers every request with
    /// `200 {"ok":true}` and keeps connections alive, counting distinct TCP
    /// connections it has accepted.
    fn spawn_echo_server() -> (String, Arc<AtomicU64>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        listener.set_nonblocking(true).unwrap();
        let listener = tokio::net::TcpListener::from_std(listener).unwrap();
        let connections = Arc::new(AtomicU64::new(0));
        let connections_task = connections.clone();
        tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    break;
                };
                connections_task.fetch_add(1, Ordering::Relaxed);
                tokio::spawn(async move {
                    let mut buf = [0u8; 8192];
                    loop {
                        let n = match socket.read(&mut buf).await {
                            Ok(0) | Err(_) => break,
                            Ok(n) => n,
                        };
                        let body = b"{\"ok\":true}";
                        let response = format!(
                            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: keep-alive\r\n\r\n",
                            body.len()
                        );
                        let mut out = response.as_bytes().to_vec();
                        out.extend_from_slice(body);
                        if socket.write_all(&out).await.is_err() || n == 0 {
                            break;
                        }
                    }
                });
            }
        });
        (format!("http://{addr}"), connections)
    }

    #[test]
    fn parses_openai_error_body() {
        let (msg, code) = parse_error_body(
            r#"{"error":{"message":"bad request","type":"invalid_request_error","code":"invalid_param"}}"#,
        );
        assert_eq!(msg, "bad request");
        assert_eq!(code.as_deref(), Some("invalid_param"));
    }

    #[test]
    fn falls_back_to_raw_text() {
        let (msg, code) = parse_error_body("<html>gateway error</html>");
        assert_eq!(msg, "<html>gateway error</html>");
        assert!(code.is_none());
    }

    #[test]
    fn parses_retry_after_from_body() {
        let d = parse_retry_after(r#"{"error":{"retry_after":2.5}}"#);
        assert_eq!(d, Some(Duration::from_secs_f64(2.5)));
    }

    #[test]
    fn parses_retry_after_header_integer_seconds() {
        assert_eq!(parse_retry_after_value("30"), Some(Duration::from_secs(30)));
        assert_eq!(
            parse_retry_after_value(" 120 "),
            Some(Duration::from_secs(120))
        );
    }

    #[test]
    fn retry_after_header_ignores_http_date_and_garbage() {
        // HTTP-date form is tolerated by being ignored.
        assert_eq!(
            parse_retry_after_value("Wed, 21 Oct 2015 07:28:00 GMT"),
            None
        );
        assert_eq!(parse_retry_after_value(""), None);
        assert_eq!(parse_retry_after_value("-3"), None);
    }

    #[test]
    fn retry_after_from_headers_reads_the_header() {
        let mut headers = reqwest::header::HeaderMap::new();
        assert_eq!(retry_after_from_headers(&headers), None);
        headers.insert("retry-after", "45".parse().unwrap());
        assert_eq!(
            retry_after_from_headers(&headers),
            Some(Duration::from_secs(45))
        );
    }

    #[tokio::test]
    async fn rate_limit_prefers_header_over_body_hint() {
        // Wire-level check: the header wins when both hints are present...
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("retry-after", "30".parse().unwrap());
        let e = map_response_error(
            "openai",
            reqwest::StatusCode::TOO_MANY_REQUESTS,
            retry_after_from_headers(&headers),
            br#"{"error":{"message":"slow","retry_after":2.5}}"#,
        )
        .await;
        match e {
            AiError::RateLimit(err) => assert_eq!(err.retry_after, Some(Duration::from_secs(30))),
            other => panic!("expected rate limit error, got {other:?}"),
        }

        // ...and the body hint is used only when no header was sent.
        let e = map_response_error(
            "openai",
            reqwest::StatusCode::TOO_MANY_REQUESTS,
            None,
            br#"{"error":{"message":"slow","retry_after":2.5}}"#,
        )
        .await;
        match e {
            AiError::RateLimit(err) => {
                assert_eq!(err.retry_after, Some(Duration::from_secs_f64(2.5)))
            }
            other => panic!("expected rate limit error, got {other:?}"),
        }
    }

    #[test]
    fn shared_returns_the_same_pool_everywhere() {
        let a = HttpClient::shared();
        let b = HttpClient::shared();
        let isolated = HttpClient::new().unwrap();
        assert!(a.shares_pool_with(&b));
        assert!(!a.shares_pool_with(&isolated));
    }

    #[tokio::test]
    async fn shared_pool_reuses_connections_across_requests() {
        let (base_url, connections) = spawn_echo_server();
        let client = HttpClient::shared();
        for _ in 0..5 {
            let builder = client
                .post(format!("{base_url}/chat/completions"))
                .header("x-test", "1")
                .body("{}");
            let response = client.execute(builder).await.unwrap();
            assert_eq!(response.status(), reqwest::StatusCode::OK);
        }
        // All five requests multiplexed over a single kept-alive connection.
        assert_eq!(connections.load(Ordering::Relaxed), 1);
        assert_eq!(client.request_count(), 5);
        assert_eq!(client.in_flight(), 0);
    }

    #[tokio::test]
    async fn execute_counts_concurrent_in_flight_requests() {
        let (base_url, _connections) = spawn_echo_server();
        let client = HttpClient::new().unwrap();
        let mut handles = Vec::new();
        for _ in 0..8 {
            let client = client.clone();
            let url = base_url.clone();
            handles.push(tokio::spawn(async move {
                let builder = client.post(url).body("{}");
                client.execute(builder).await.unwrap();
            }));
        }
        for handle in handles {
            handle.await.unwrap();
        }
        assert_eq!(client.request_count(), 8);
        assert_eq!(client.in_flight(), 0);
    }

    #[tokio::test]
    async fn maps_status_codes() {
        let e = map_response_error(
            "openai",
            reqwest::StatusCode::UNAUTHORIZED,
            None,
            b"{\"error\":{\"message\":\"nope\"}}",
        )
        .await;
        assert!(matches!(e, AiError::Authentication(_)));

        let e = map_response_error(
            "openai",
            reqwest::StatusCode::TOO_MANY_REQUESTS,
            None,
            b"{\"error\":{\"message\":\"slow\"}}",
        )
        .await;
        assert!(matches!(e, AiError::RateLimit(_)));

        let e = map_response_error(
            "openai",
            reqwest::StatusCode::BAD_REQUEST,
            None,
            b"{\"error\":{\"message\":\"bad\"}}",
        )
        .await;
        assert!(matches!(e, AiError::Provider(_)));
        assert!(!e.is_retryable());

        let e = map_response_error(
            "openai",
            reqwest::StatusCode::SERVICE_UNAVAILABLE,
            None,
            b"{\"error\":{\"message\":\"busy\"}}",
        )
        .await;
        assert!(e.is_retryable());
    }
}
