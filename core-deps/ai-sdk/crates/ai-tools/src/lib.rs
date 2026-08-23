//! Tool framework (spec §10): typed JSON-Schema tools, validation,
//! permissions, execution IDs/tracing hooks, built-in tools, and a skills
//! registry (PRD §3.3).

/// Legacy simulated browser tool; superseded by `ai-computer`'s
/// OmniChrome-backed real browser control.
#[allow(deprecated)]
pub mod browser;
mod builtins;
mod math;
mod registry;
mod schema;

#[allow(deprecated)]
pub use browser::{BrowserAction, BrowserTool};
pub use builtins::{HttpTool, MathTool, TimeTool, UuidTool};
pub use math::evaluate_expression;
pub use registry::{Skill, SkillRegistry, ToolRegistry};
pub use schema::{ArgumentError, validate_arguments};

use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use ai_errors::{AiError, ToolError};

/// The outcome of a tool execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolOutput {
    /// JSON-encoded result (or error message when `is_error`).
    pub content: String,
    pub is_error: bool,
}

impl ToolOutput {
    pub fn ok(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            is_error: false,
        }
    }

    pub fn error(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            is_error: true,
        }
    }
}

/// Context handed to tools on execution (permissions, budget, trace id).
#[derive(Debug, Clone, Default)]
pub struct ToolContext {
    /// Permission gate; tools deny operations not explicitly allowed.
    pub permissions: ai_security::Permissions,
    /// Execution/trace id for observability correlation.
    pub execution_id: Option<String>,
    /// Wall-clock deadline for the tool (None = no deadline).
    pub deadline: Option<std::time::Instant>,
    /// Maximum response bytes (None = unbounded; used by web tools).
    pub max_response_bytes: Option<usize>,
}

/// A tool callable by agents and models.
#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    /// JSON Schema for the input arguments.
    fn input_schema(&self) -> serde_json::Value;
    /// Permission strings required (e.g. `"net:http"`, `"fs:read"`).
    fn required_permissions(&self) -> Vec<&str> {
        Vec::new()
    }
    /// Executes the tool with validated arguments.
    async fn execute(
        &self,
        arguments: serde_json::Value,
        context: &ToolContext,
    ) -> Result<ToolOutput, AiError>;
}

/// Validates arguments against a tool's schema, then executes it with
/// permission checks. Produces typed [`ToolError`]s on any failure.
pub async fn run_tool(
    tool: &dyn Tool,
    arguments: serde_json::Value,
    context: &ToolContext,
) -> Result<ToolOutput, AiError> {
    let name = tool.name().to_string();
    validate_arguments(&tool.input_schema(), &arguments).map_err(|e| {
        AiError::Tool(ToolError::new(
            &name,
            format!("argument validation failed: {e}"),
        ))
    })?;
    for permission in tool.required_permissions() {
        if !context.permissions.permits(permission) {
            return Err(AiError::Tool(ToolError::new(
                &name,
                format!("permission `{permission}` not granted"),
            )));
        }
    }
    let result = tool.execute(arguments, context).await;
    result.map_err(|e| AiError::Tool(ToolError::with_source(&name, "tool execution failed", e)))
}

/// A dynamically-defined tool (closures), for user-defined tools.
pub struct FunctionTool {
    name: String,
    description: String,
    schema: serde_json::Value,
    permissions: Vec<String>,
    handler: Box<dyn Fn(serde_json::Value) -> Result<ToolOutput, AiError> + Send + Sync>,
}

impl FunctionTool {
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        schema: serde_json::Value,
        handler: impl Fn(serde_json::Value) -> Result<ToolOutput, AiError> + Send + Sync + 'static,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            schema,
            permissions: Vec::new(),
            handler: Box::new(handler),
        }
    }

    pub fn with_permission(mut self, permission: &str) -> Self {
        self.permissions.push(permission.to_string());
        self
    }
}

#[async_trait]
impl Tool for FunctionTool {
    fn name(&self) -> &str {
        &self.name
    }
    fn description(&self) -> &str {
        &self.description
    }
    fn input_schema(&self) -> serde_json::Value {
        self.schema.clone()
    }
    fn required_permissions(&self) -> Vec<&str> {
        self.permissions.iter().map(|s| s.as_str()).collect()
    }
    async fn execute(
        &self,
        arguments: serde_json::Value,
        _context: &ToolContext,
    ) -> Result<ToolOutput, AiError> {
        (self.handler)(arguments)
    }
}

/// Convenience: builds a [`ToolDefinition`] (for model requests) from a tool.
pub fn to_tool_definition(tool: &dyn Tool) -> ai_core::ToolDefinition {
    ai_core::ToolDefinition::new(tool.name(), tool.description(), tool.input_schema())
}

/// All built-in tools as a ready registry.
///
/// The simulated `browser_action` remains registered for compatibility —
/// prefer `ai_computer`'s real browser/desktop plugins in production.
#[allow(deprecated)]
pub fn default_tools() -> ToolRegistry {
    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(HttpTool::default()));
    registry.register(Arc::new(TimeTool));
    registry.register(Arc::new(MathTool));
    registry.register(Arc::new(UuidTool));
    registry.register(Arc::new(BrowserTool::new()));
    registry
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn function_tool_runs_with_validation() {
        let tool = FunctionTool::new(
            "double",
            "Doubles a number",
            serde_json::json!({
                "type": "object",
                "properties": {"value": {"type": "number"}},
                "required": ["value"]
            }),
            |args| {
                let value = args["value"]
                    .as_f64()
                    .ok_or_else(|| AiError::Tool(ToolError::new("double", "missing value")))?;
                Ok(ToolOutput::ok(format!("{}", value * 2.0)))
            },
        );
        let context = ToolContext::default();
        let output = run_tool(&tool, serde_json::json!({"value": 21}), &context)
            .await
            .unwrap();
        assert_eq!(output.content, "42");

        // Missing required argument → typed validation error.
        let err = run_tool(&tool, serde_json::json!({}), &context)
            .await
            .unwrap_err();
        assert!(matches!(err, AiError::Tool(_)), "{err}");
    }

    #[tokio::test]
    async fn permissions_are_enforced() {
        let tool = FunctionTool::new(
            "fs_read",
            "reads a file",
            serde_json::json!({"type": "object", "properties": {}}),
            |_| Ok(ToolOutput::ok("content")),
        )
        .with_permission("fs:read");
        let context = ToolContext::default(); // no permissions granted
        let err = run_tool(&tool, serde_json::json!({}), &context)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("fs:read"), "{err}");

        let context = ToolContext {
            permissions: ai_security::Permissions::new().allow("fs:read"),
            ..Default::default()
        };
        let output = run_tool(&tool, serde_json::json!({}), &context)
            .await
            .unwrap();
        assert_eq!(output.content, "content");
    }

    #[tokio::test]
    async fn math_tool_evaluates() {
        let tool = MathTool;
        let context = ToolContext::default();
        let output = run_tool(&tool, serde_json::json!({"expression": "6 * 7"}), &context)
            .await
            .unwrap();
        let value: serde_json::Value = serde_json::from_str(&output.content).unwrap();
        assert_eq!(value["result"], serde_json::json!(42.0));
        let output = run_tool(
            &tool,
            serde_json::json!({"expression": "(2 + 3) * 4"}),
            &context,
        )
        .await
        .unwrap();
        let value: serde_json::Value = serde_json::from_str(&output.content).unwrap();
        assert_eq!(value["result"], serde_json::json!(20.0));
    }

    #[tokio::test]
    async fn time_tool_returns_rfc3339() {
        let tool = TimeTool;
        let context = ToolContext::default();
        let output = run_tool(&tool, serde_json::json!({"format": "rfc3339"}), &context)
            .await
            .unwrap();
        let value: serde_json::Value = serde_json::from_str(&output.content).unwrap();
        assert!(value["now"].as_str().unwrap().contains('T'), "{}", value);
    }

    #[tokio::test]
    async fn uuid_tool_generates_v4() {
        let tool = UuidTool;
        let context = ToolContext::default();
        let output = run_tool(&tool, serde_json::json!({}), &context)
            .await
            .unwrap();
        let value: serde_json::Value = serde_json::from_str(&output.content).unwrap();
        let uuid = value["uuid"].as_str().unwrap();
        assert_eq!(uuid.len(), 36);
        assert_eq!(&uuid[14..15], "4", "v4 marker");
    }
}
