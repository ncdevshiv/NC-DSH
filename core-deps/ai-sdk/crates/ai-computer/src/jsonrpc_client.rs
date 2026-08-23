//! Authenticated JSON-RPC 2.0 over HTTP — the shared transport seam for
//! both automation plugins.
//!
//! Encodes the wire quirks documented by protocol recon:
//!
//! - OmniChrome (`:8765/rpc`): Bearer token; HTTP 401 wraps `-32001`;
//!   forwarding failures surface as HTTP 500 wrapping `-32000` (extension
//!   timeout or "no extension attached"); `result` on success.
//! - NativeServer (`:8888/rpc`): exact-string `Authorization: Bearer`;
//!   handler exceptions return HTTP-level success with a JSON-RPC `error`
//!   body whose `id` is ALWAYS null; unknown top-level members (the
//!   non-standard `agent` echo) must be tolerated.
//!
//! Callers correlate requests by awaiting each call (single-flight per
//! client), which sidesteps computeruse's null-id error correlation gap.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use serde_json::Value;

/// Errors surfaced by the automation plugins, mapped onto typed
/// [`AiError`]s at the tool layer.
#[derive(Debug, Clone)]
pub enum ComputerError {
    /// Bad/missing token (HTTP 401 / `-32001`).
    Unauthorized(String),
    /// The engine cannot be reached or is not attached (connection refused,
    /// bridge without Chrome extension, HTTP 500 `-32000`).
    EngineUnreachable(String),
    /// Engine-side rejection (JSON-RPC `error` object).
    Rpc { code: i64, message: String },
    /// Request timed out locally.
    Timeout(String),
    /// Arguments failed client-side validation (never sent on the wire).
    InvalidArgs(String),
}

impl std::fmt::Display for ComputerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unauthorized(m) => write!(f, "unauthorized: {m}"),
            Self::EngineUnreachable(m) => write!(f, "engine unreachable: {m}"),
            Self::Rpc { code, message } => write!(f, "rpc error {code}: {message}"),
            Self::Timeout(m) => write!(f, "timeout: {m}"),
            Self::InvalidArgs(m) => write!(f, "invalid arguments: {m}"),
        }
    }
}

impl std::error::Error for ComputerError {}

impl From<ComputerError> for ai_errors::AiError {
    fn from(e: ComputerError) -> Self {
        use ai_errors::{AiError, ProviderError, ToolError};
        let msg = e.to_string();
        match &e {
            ComputerError::InvalidArgs(_) | ComputerError::Unauthorized(_) => {
                AiError::Tool(ToolError::new("computer", msg))
            }
            _ => AiError::Provider(ProviderError::new("computer", msg)),
        }
    }
}

/// Resolves a token: explicit value → environment variable → token file.
/// Empty/whitespace values count as unset. Missing everywhere yields `None`
/// (callers decide whether that is fatal).
pub fn resolve_token(
    explicit: Option<String>,
    env_key: &str,
    file_path: Option<&std::path::Path>,
) -> Option<String> {
    if let Some(t) = explicit
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
    {
        return Some(t);
    }
    if let Ok(t) = std::env::var(env_key) {
        let t = t.trim().to_string();
        if !t.is_empty() {
            return Some(t);
        }
    }
    file_path
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
}

/// JSON-RPC 2.0 over HTTP POST with optional bearer auth.
pub struct JsonRpcHttpClient {
    endpoint: String,
    token: Option<String>,
    http: reqwest::Client,
    next_id: AtomicU64,
    timeout: Duration,
}

impl Clone for JsonRpcHttpClient {
    fn clone(&self) -> Self {
        Self {
            endpoint: self.endpoint.clone(),
            token: self.token.clone(),
            http: self.http.clone(),
            // Clones continue the id sequence rather than sharing the
            // counter — ids only need uniqueness per logical stream.
            next_id: AtomicU64::new(self.next_id.load(Ordering::Relaxed)),
            timeout: self.timeout,
        }
    }
}

impl JsonRpcHttpClient {
    pub fn new(endpoint: impl Into<String>, token: Option<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            token,
            http: reqwest::Client::new(),
            next_id: AtomicU64::new(1),
            timeout: Duration::from_secs(35),
        }
    }

    /// Local override of the per-call timeout (must exceed OmniChrome's
    /// 30 s bridge-forwarding cap to surface *its* timeout message).
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    /// Executes one call and returns the `result` member.
    ///
    /// Error mapping precedence: local timeout → HTTP status (401 ⇒
    /// [`ComputerError::Unauthorized`]) → body-level `error` object
    /// (regardless of HTTP status — NativeServer reports handler failures
    /// this way) → transport failure ⇒ [`ComputerError::EngineUnreachable`].
    pub async fn call(&self, method: &str, params: Value) -> Result<Value, ComputerError> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let envelope = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });

        let mut req = self
            .http
            .post(&self.endpoint)
            .timeout(self.timeout)
            .json(&envelope);
        if let Some(token) = &self.token {
            req = req.bearer_auth(token);
        }

        let response = req.send().await.map_err(|e| {
            if e.is_timeout() {
                ComputerError::Timeout(format!("{method} exceeded {:?}", self.timeout))
            } else {
                ComputerError::EngineUnreachable(format!(
                    "{method}: cannot reach {endpoint} ({e}); is the engine running?",
                    endpoint = self.endpoint
                ))
            }
        })?;

        let status = response.status();
        let body: Value = response.json().await.map_err(|e| {
            ComputerError::EngineUnreachable(format!("{method}: undecodable response ({e})"))
        })?;

        // Body-level error wins over status mapping (NativeServer returns
        // handler failures with id:null inside otherwise-successful HTTP).
        if let Some(err) = body.get("error") {
            let code = err.get("code").and_then(|c| c.as_i64()).unwrap_or(-32000);
            let message = err
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("unknown engine error")
                .to_string();
            return Err(match (status.as_u16(), code) {
                (401, _) | (_, -32001) => ComputerError::Unauthorized(message),
                _ => ComputerError::Rpc { code, message },
            });
        }

        if status.as_u16() == 401 {
            return Err(ComputerError::Unauthorized(
                "HTTP 401 from engine (bad or missing token)".into(),
            ));
        }
        if !status.is_success() {
            return Err(ComputerError::EngineUnreachable(format!(
                "{method}: engine returned HTTP {status}: {}",
                brief(&body)
            )));
        }

        Ok(body.get("result").cloned().unwrap_or(Value::Null))
    }
}

/// Compact single-line rendering of an arbitrary body for error messages.
fn brief(v: &Value) -> String {
    let s = v.to_string();
    let cut = s.char_indices().nth(160).map(|(i, _)| i).unwrap_or(s.len());
    let mut out = s[..cut].to_string();
    if cut < s.len() {
        out.push('…');
    }
    out
}

/// Case-tolerant field reader for engines with inconsistent result casing
/// (`clickedAt` vs `Success`): tries each candidate verbatim.
pub fn field<'a>(value: &'a Value, names: &[&str]) -> Option<&'a Value> {
    let obj = value.as_object()?;
    names.iter().find_map(|n| obj.get(*n))
}
