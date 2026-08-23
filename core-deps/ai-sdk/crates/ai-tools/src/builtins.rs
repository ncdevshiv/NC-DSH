//! Built-in tools: HTTP fetch (SSRF-guarded), time, math, uuid.

use async_trait::async_trait;

use ai_errors::{AiError, WebError};

use crate::{Tool, ToolContext, ToolOutput};

/// Fetches a URL's text content through an SSRF guard (spec §20).
///
/// Permission: `net:http`. Arguments: `{ "url": "...", "max_bytes": N }`.
pub struct HttpTool {
    policy: ai_security::UrlPolicy,
    client: reqwest::Client,
}

impl Default for HttpTool {
    fn default() -> Self {
        Self::new(ai_security::UrlPolicy::new())
    }
}

impl HttpTool {
    pub fn new(policy: ai_security::UrlPolicy) -> Self {
        Self {
            policy,
            client: reqwest::Client::builder()
                .user_agent("ai-sdk-tools/0.1")
                .build()
                .expect("reqwest client builds"),
        }
    }
}

#[async_trait]
impl Tool for HttpTool {
    fn name(&self) -> &str {
        "http_get"
    }

    fn description(&self) -> &str {
        "Fetches a URL and returns its text content (SSRF-guarded, max 512 KB)"
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "url": {"type": "string", "description": "http(s) URL to fetch"},
                "max_bytes": {"type": "integer", "default": 524288}
            },
            "required": ["url"]
        })
    }

    fn required_permissions(&self) -> Vec<&str> {
        vec!["net:http"]
    }

    async fn execute(
        &self,
        arguments: serde_json::Value,
        context: &ToolContext,
    ) -> Result<ToolOutput, AiError> {
        let url = arguments["url"]
            .as_str()
            .ok_or_else(|| AiError::Tool(ai_errors::ToolError::new("http_get", "missing `url`")))?;

        self.policy.require(url)?;

        let max_bytes = arguments
            .get("max_bytes")
            .and_then(|v| v.as_u64())
            .unwrap_or(524_288)
            .min(context.max_response_bytes.unwrap_or(524_288) as u64)
            as usize;

        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| AiError::Web(WebError::new("http_get", e.to_string())))?;

        let status = response.status();
        let body = response
            .bytes()
            .await
            .map_err(|e| AiError::Web(WebError::new("http_get", e.to_string())))?;

        if body.len() > max_bytes {
            return Err(AiError::Web(WebError::new(
                "http_get",
                format!("response exceeds {max_bytes} bytes"),
            )));
        }

        let content = String::from_utf8_lossy(&body).into_owned();
        let result = serde_json::json!({
            "status": status.as_u16(),
            "content_type": "text/plain",
            "body": content,
        });
        Ok(ToolOutput::ok(
            serde_json::to_string(&result).unwrap_or_else(|_| "{}".into()),
        ))
    }
}

/// Returns the current time in the requested format.
pub struct TimeTool;

impl Default for TimeTool {
    fn default() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for TimeTool {
    fn name(&self) -> &str {
        "time"
    }

    fn description(&self) -> &str {
        "Returns the current time as RFC 3339 or Unix seconds"
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "format": {"type": "string", "enum": ["rfc3339", "unix"], "default": "rfc3339"}
            }
        })
    }

    async fn execute(
        &self,
        arguments: serde_json::Value,
        _context: &ToolContext,
    ) -> Result<ToolOutput, AiError> {
        let format = arguments
            .get("format")
            .and_then(|v| v.as_str())
            .unwrap_or("rfc3339");
        let result = match format {
            "unix" => serde_json::json!({
                "now": time::OffsetDateTime::now_utc().unix_timestamp()
            }),
            _ => {
                let now = time::OffsetDateTime::now_utc();
                let formatted = now
                    .format(&time::format_description::well_known::Rfc3339)
                    .unwrap_or_else(|_| "unknown".to_string());
                serde_json::json!({"now": formatted})
            }
        };
        Ok(ToolOutput::ok(
            serde_json::to_string(&result).unwrap_or_else(|_| "{}".into()),
        ))
    }
}

/// Evaluates an arithmetic expression (safe evaluator, no eval()).
pub struct MathTool;

impl Default for MathTool {
    fn default() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for MathTool {
    fn name(&self) -> &str {
        "calculator"
    }

    fn description(&self) -> &str {
        "Evaluates a simple arithmetic expression like '6 * 7' and returns the numeric result"
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "expression": {"type": "string", "description": "Arithmetic expression"}
            },
            "required": ["expression"]
        })
    }

    async fn execute(
        &self,
        arguments: serde_json::Value,
        _context: &ToolContext,
    ) -> Result<ToolOutput, AiError> {
        let expression = arguments["expression"].as_str().ok_or_else(|| {
            AiError::Tool(ai_errors::ToolError::new(
                "calculator",
                "missing `expression`",
            ))
        })?;
        match crate::evaluate_expression(expression) {
            Ok(value) => Ok(ToolOutput::ok(
                serde_json::json!({"result": value}).to_string(),
            )),
            Err(e) => Ok(ToolOutput::error(format!("invalid expression: {e}"))),
        }
    }
}

/// Generates a UUID v4.
pub struct UuidTool;

impl Default for UuidTool {
    fn default() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for UuidTool {
    fn name(&self) -> &str {
        "uuid"
    }

    fn description(&self) -> &str {
        "Generates a random UUID v4"
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({"type": "object", "properties": {}})
    }

    async fn execute(
        &self,
        _arguments: serde_json::Value,
        _context: &ToolContext,
    ) -> Result<ToolOutput, AiError> {
        let uuid = uuid::Uuid::new_v4().to_string();
        Ok(ToolOutput::ok(
            serde_json::json!({"uuid": uuid}).to_string(),
        ))
    }
}
