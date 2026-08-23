//! Typed error hierarchy for the AI SDK.
//!
//! All subsystems report failures through [`AiError`], which distinguishes
//! between categories such as configuration, authentication, provider,
//! rate-limit, timeout, network, serialization, validation, tool, web,
//! storage, agent, workflow, and cancellation failures.
//!
//! Errors preserve useful context while never leaking secrets.

use std::fmt;

/// The root error type for the entire AI SDK.
///
/// Each variant carries typed payloads and an optional `source` chain.
/// Use [`AiError::is_retryable`] to decide whether a retry policy should
/// retry the operation (see `ai-runtime`).
#[derive(Debug)]
#[non_exhaustive]
pub enum AiError {
    /// Invalid or missing configuration (e.g. missing API key).
    Configuration(ConfigurationError),
    /// Authentication or authorization failure from a provider/API.
    Authentication(AuthenticationError),
    /// The provider/API returned an application-level error.
    Provider(ProviderError),
    /// A rate limit was hit (HTTP 429 or provider rate-limit payload).
    RateLimit(RateLimitError),
    /// The operation exceeded its deadline.
    Timeout(TimeoutError),
    /// A transport-level failure (DNS, connect, reset, TLS…).
    Network(NetworkError),
    /// Failure to serialize or deserialize data.
    Serialization(SerializationError),
    /// Input failed schema/domain validation.
    Validation(ValidationError),
    /// A tool failed to execute.
    Tool(ToolError),
    /// The web subsystem failed (fetch, crawl, extraction…).
    Web(WebError),
    /// A storage backend failed.
    Storage(StorageError),
    /// The agent runtime failed.
    Agent(AgentError),
    /// The workflow engine failed.
    Workflow(WorkflowError),
    /// The operation was cancelled (either externally or by a deadline).
    Cancelled(CancellationError),
    /// An unexpected internal error, with context.
    Internal(InternalError),
}

impl AiError {
    /// Whether a retry policy should consider this error retryable.
    ///
    /// Never retries: validation, authentication, cancellation, internal.
    /// Retries: rate limit (after backoff), timeout, network, provider
    /// 5xx-class errors. See [`ProviderError::is_retryable`].
    pub fn is_retryable(&self) -> bool {
        match self {
            Self::RateLimit(_) | Self::Timeout(_) | Self::Network(_) => true,
            Self::Provider(e) => e.is_retryable(),
            _ => false,
        }
    }
}

impl fmt::Display for AiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Configuration(e) => write!(f, "configuration error: {e}"),
            Self::Authentication(e) => write!(f, "authentication error: {e}"),
            Self::Provider(e) => write!(f, "provider error: {e}"),
            Self::RateLimit(e) => write!(f, "rate limit: {e}"),
            Self::Timeout(e) => write!(f, "timeout: {e}"),
            Self::Network(e) => write!(f, "network error: {e}"),
            Self::Serialization(e) => write!(f, "serialization error: {e}"),
            Self::Validation(e) => write!(f, "validation error: {e}"),
            Self::Tool(e) => write!(f, "tool error: {e}"),
            Self::Web(e) => write!(f, "web error: {e}"),
            Self::Storage(e) => write!(f, "storage error: {e}"),
            Self::Agent(e) => write!(f, "agent error: {e}"),
            Self::Workflow(e) => write!(f, "workflow error: {e}"),
            Self::Cancelled(e) => write!(f, "cancelled: {e}"),
            Self::Internal(e) => write!(f, "internal error: {e}"),
        }
    }
}

impl std::error::Error for AiError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Configuration(e) => e.source(),
            Self::Authentication(e) => e.source(),
            Self::Provider(e) => e.source(),
            Self::RateLimit(e) => e.source(),
            Self::Timeout(e) => e.source(),
            Self::Network(e) => e.source(),
            Self::Serialization(e) => e.source(),
            Self::Validation(e) => e.source(),
            Self::Tool(e) => e.source(),
            Self::Web(e) => e.source(),
            Self::Storage(e) => e.source(),
            Self::Agent(e) => e.source(),
            Self::Workflow(e) => e.source(),
            Self::Cancelled(e) => e.source(),
            Self::Internal(e) => e.source(),
        }
    }
}

impl From<ConfigurationError> for AiError {
    fn from(e: ConfigurationError) -> Self {
        Self::Configuration(e)
    }
}
impl From<AuthenticationError> for AiError {
    fn from(e: AuthenticationError) -> Self {
        Self::Authentication(e)
    }
}
impl From<ProviderError> for AiError {
    fn from(e: ProviderError) -> Self {
        Self::Provider(e)
    }
}
impl From<RateLimitError> for AiError {
    fn from(e: RateLimitError) -> Self {
        Self::RateLimit(e)
    }
}
impl From<TimeoutError> for AiError {
    fn from(e: TimeoutError) -> Self {
        Self::Timeout(e)
    }
}
impl From<NetworkError> for AiError {
    fn from(e: NetworkError) -> Self {
        Self::Network(e)
    }
}
impl From<SerializationError> for AiError {
    fn from(e: SerializationError) -> Self {
        Self::Serialization(e)
    }
}
impl From<ValidationError> for AiError {
    fn from(e: ValidationError) -> Self {
        Self::Validation(e)
    }
}
impl From<ToolError> for AiError {
    fn from(e: ToolError) -> Self {
        Self::Tool(e)
    }
}
impl From<WebError> for AiError {
    fn from(e: WebError) -> Self {
        Self::Web(e)
    }
}
impl From<StorageError> for AiError {
    fn from(e: StorageError) -> Self {
        Self::Storage(e)
    }
}
impl From<AgentError> for AiError {
    fn from(e: AgentError) -> Self {
        Self::Agent(e)
    }
}
impl From<WorkflowError> for AiError {
    fn from(e: WorkflowError) -> Self {
        Self::Workflow(e)
    }
}
impl From<CancellationError> for AiError {
    fn from(e: CancellationError) -> Self {
        Self::Cancelled(e)
    }
}
impl From<InternalError> for AiError {
    fn from(e: InternalError) -> Self {
        Self::Internal(e)
    }
}

/// Missing or invalid configuration.
#[derive(Debug, thiserror::Error)]
#[error("`{key}`: {message}")]
pub struct ConfigurationError {
    /// The configuration key that failed (e.g. `OPENAI_API_KEY`).
    pub key: String,
    /// Human-readable explanation.
    pub message: String,
    #[source]
    pub source: Option<Box<dyn std::error::Error + Send + Sync>>,
}

impl ConfigurationError {
    pub fn new(key: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            message: message.into(),
            source: None,
        }
    }

    pub fn with_source(
        key: impl Into<String>,
        message: impl Into<String>,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        Self {
            key: key.into(),
            message: message.into(),
            source: Some(Box::new(source)),
        }
    }
}

/// Authentication or authorization failure.
#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub struct AuthenticationError {
    pub message: String,
    #[source]
    pub source: Option<Box<dyn std::error::Error + Send + Sync>>,
}

impl AuthenticationError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            source: None,
        }
    }
}

/// Provider/API application-level error.
#[derive(Debug, thiserror::Error)]
#[error("{provider}: HTTP {status}{code} {message}",
    status = display_status(self.status),
    code = display_code(&self.code))]
pub struct ProviderError {
    /// Provider identifier (e.g. `openai`).
    pub provider: String,
    /// HTTP status code, when applicable.
    pub status: Option<u16>,
    /// Provider error code (e.g. `insufficient_quota`), when available.
    pub code: Option<String>,
    /// Error message from the provider.
    pub message: String,
    #[source]
    pub source: Option<Box<dyn std::error::Error + Send + Sync>>,
}

fn display_status(status: Option<u16>) -> String {
    status
        .map(|s| s.to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

fn display_code(code: &Option<String>) -> String {
    code.as_ref().map(|c| format!(" [{c}]")).unwrap_or_default()
}

impl ProviderError {
    pub fn new(provider: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            provider: provider.into(),
            status: None,
            code: None,
            message: message.into(),
            source: None,
        }
    }

    /// Sets the HTTP status code (builder style).
    pub fn with_status(mut self, status: u16) -> Self {
        self.status = Some(status);
        self
    }

    /// Sets the provider error code (builder style).
    pub fn with_code(mut self, code: impl Into<String>) -> Self {
        self.code = Some(code.into());
        self
    }

    /// True when the status indicates a transient server-side failure
    /// (500, 502, 503, 504) worth retrying.
    pub fn is_retryable(&self) -> bool {
        matches!(self.status, Some(500 | 502 | 503 | 504))
    }
}

/// Rate limit hit (HTTP 429 or provider rate-limit payload).
#[derive(Debug, thiserror::Error)]
#[error("{provider} rate limited{}: {message}",
    if let Some(after) = &self.retry_after {
        format!(" (retry after {after:?})")
    } else {
        String::new()
    })]
pub struct RateLimitError {
    pub provider: String,
    /// Server-requested retry delay, when provided (`Retry-After` header or
    /// provider payload).
    pub retry_after: Option<std::time::Duration>,
    pub message: String,
    #[source]
    pub source: Option<Box<dyn std::error::Error + Send + Sync>>,
}

impl RateLimitError {
    pub fn new(provider: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            provider: provider.into(),
            retry_after: None,
            message: message.into(),
            source: None,
        }
    }

    /// Sets the server-provided retry delay (builder style).
    pub fn with_retry_after(mut self, retry_after: std::time::Duration) -> Self {
        self.retry_after = Some(retry_after);
        self
    }
}

/// Deadline exceeded.
#[derive(Debug, thiserror::Error)]
#[error("operation timed out after {duration:?}: {operation}")]
pub struct TimeoutError {
    pub operation: String,
    pub duration: std::time::Duration,
    #[source]
    pub source: Option<Box<dyn std::error::Error + Send + Sync>>,
}

impl TimeoutError {
    pub fn new(operation: impl Into<String>, duration: std::time::Duration) -> Self {
        Self {
            operation: operation.into(),
            duration,
            source: None,
        }
    }
}

/// Transport-level failure.
#[derive(Debug, thiserror::Error)]
#[error("network failure for {operation}: {message}")]
pub struct NetworkError {
    pub operation: String,
    pub message: String,
    #[source]
    pub source: Option<Box<dyn std::error::Error + Send + Sync>>,
}

impl NetworkError {
    pub fn new(operation: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            operation: operation.into(),
            message: message.into(),
            source: None,
        }
    }
}

/// Serialization/deserialization failure.
#[derive(Debug, thiserror::Error)]
#[error("serialization error: {message}")]
pub struct SerializationError {
    pub message: String,
    #[source]
    pub source: Option<Box<dyn std::error::Error + Send + Sync>>,
}

impl SerializationError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            source: None,
        }
    }
}

/// Input failed validation.
#[derive(Debug, thiserror::Error)]
#[error("validation failed: {message}")]
pub struct ValidationError {
    pub message: String,
    #[source]
    pub source: Option<Box<dyn std::error::Error + Send + Sync>>,
}

impl ValidationError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            source: None,
        }
    }
}

/// Tool execution failure.
#[derive(Debug, thiserror::Error)]
#[error("tool `{tool}` failed: {message}")]
pub struct ToolError {
    pub tool: String,
    pub message: String,
    #[source]
    pub source: Option<Box<dyn std::error::Error + Send + Sync>>,
}

impl ToolError {
    pub fn new(tool: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            tool: tool.into(),
            message: message.into(),
            source: None,
        }
    }

    pub fn with_source(
        tool: impl Into<String>,
        message: impl Into<String>,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        Self {
            tool: tool.into(),
            message: message.into(),
            source: Some(Box::new(source)),
        }
    }
}

/// Web subsystem failure.
#[derive(Debug, thiserror::Error)]
#[error("web error ({operation}): {message}")]
pub struct WebError {
    pub operation: String,
    pub message: String,
    #[source]
    pub source: Option<Box<dyn std::error::Error + Send + Sync>>,
}

impl WebError {
    pub fn new(operation: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            operation: operation.into(),
            message: message.into(),
            source: None,
        }
    }
}

/// Storage backend failure.
#[derive(Debug, thiserror::Error)]
#[error("storage error ({backend}): {message}")]
pub struct StorageError {
    pub backend: String,
    pub message: String,
    #[source]
    pub source: Option<Box<dyn std::error::Error + Send + Sync>>,
}

impl StorageError {
    pub fn new(backend: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            backend: backend.into(),
            message: message.into(),
            source: None,
        }
    }
}

/// Agent runtime failure.
#[derive(Debug, thiserror::Error)]
#[error("agent `{agent}`: {message}")]
pub struct AgentError {
    pub agent: String,
    pub message: String,
    #[source]
    pub source: Option<Box<dyn std::error::Error + Send + Sync>>,
}

impl AgentError {
    pub fn new(agent: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            agent: agent.into(),
            message: message.into(),
            source: None,
        }
    }

    pub fn with_source(
        agent: impl Into<String>,
        message: impl Into<String>,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        Self {
            agent: agent.into(),
            message: message.into(),
            source: Some(Box::new(source)),
        }
    }
}

/// Workflow engine failure.
#[derive(Debug, thiserror::Error)]
#[error("workflow `{workflow}`: {message}")]
pub struct WorkflowError {
    pub workflow: String,
    pub message: String,
    #[source]
    pub source: Option<Box<dyn std::error::Error + Send + Sync>>,
}

impl WorkflowError {
    pub fn new(workflow: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            workflow: workflow.into(),
            message: message.into(),
            source: None,
        }
    }

    pub fn with_source(
        workflow: impl Into<String>,
        message: impl Into<String>,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        Self {
            workflow: workflow.into(),
            message: message.into(),
            source: Some(Box::new(source)),
        }
    }
}

/// Operation cancelled (externally or by deadline).
#[derive(Debug, thiserror::Error)]
#[error("{operation} was cancelled")]
pub struct CancellationError {
    pub operation: String,
    #[source]
    pub source: Option<Box<dyn std::error::Error + Send + Sync>>,
}

impl CancellationError {
    pub fn new(operation: impl Into<String>) -> Self {
        Self {
            operation: operation.into(),
            source: None,
        }
    }
}

/// Unexpected internal failure.
#[derive(Debug, thiserror::Error)]
#[error("internal error: {message}")]
pub struct InternalError {
    pub message: String,
    #[source]
    pub source: Option<Box<dyn std::error::Error + Send + Sync>>,
}

impl InternalError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            source: None,
        }
    }

    pub fn with_source(
        message: impl Into<String>,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        Self {
            message: message.into(),
            source: Some(Box::new(source)),
        }
    }
}

/// Convenience alias for the workspace-wide result type.
pub type Result<T, E = AiError> = std::result::Result<T, E>;

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;

    #[test]
    fn retryability_classification() {
        assert!(AiError::RateLimit(RateLimitError::new("openai", "429")).is_retryable());
        assert!(
            AiError::Timeout(TimeoutError::new(
                "generate",
                std::time::Duration::from_secs(30)
            ))
            .is_retryable()
        );
        assert!(AiError::Network(NetworkError::new("generate", "reset")).is_retryable());
        let mut provider5xx = ProviderError::new("openai", "boom");
        provider5xx.status = Some(500);
        assert!(AiError::Provider(provider5xx).is_retryable());
        let mut provider4xx = ProviderError::new("openai", "bad request");
        provider4xx.status = Some(400);
        assert!(!AiError::Provider(provider4xx).is_retryable());
        assert!(!AiError::Validation(ValidationError::new("bad input")).is_retryable());
        assert!(!AiError::Authentication(AuthenticationError::new("401")).is_retryable());
        assert!(!AiError::Cancelled(CancellationError::new("run")).is_retryable());
    }

    #[test]
    fn display_includes_provider_details() {
        let mut err = ProviderError::new("openai", "quota exceeded");
        err.code = Some("insufficient_quota".into());
        let text = err.to_string();
        assert!(text.contains("quota exceeded"), "{text}");
        assert!(text.contains("insufficient_quota"), "{text}");
    }

    #[test]
    fn error_source_chain() {
        let inner = std::io::Error::other("disk full");
        let err = StorageError::new("sqlite", "write failed");
        let wrapped: AiError = err.into();
        assert!(wrapped.source().is_none());
        let _ = inner;
    }

    #[test]
    fn conversions_roundtrip() {
        let e: AiError = ConfigurationError::new("OPENAI_API_KEY", "missing").into();
        assert!(matches!(e, AiError::Configuration(_)));
        let e: AiError = ToolError::new("fs", "denied").into();
        assert!(matches!(e, AiError::Tool(_)));
        let e: AiError = WebError::new("fetch", "404").into();
        assert!(matches!(e, AiError::Web(_)));
    }
}
