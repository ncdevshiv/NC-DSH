//! Simulated browser tool — validates URLs against SSRF policy and returns
//! acknowledgement strings WITHOUT driving any browser; wire to a real
//! engine (e.g., chromiumoxide/CDP) before trusting outputs.
//!
//! Provides structured tool interfaces for web browser actions (navigate,
//! click, type text, screenshot) with SSRF URL policy validation. Only
//! `Navigate` performs a real check (the SSRF URL policy); every other
//! action merely echoes back a success string, so `execute()` results must
//! not be treated as evidence that any browser interaction occurred.

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{Tool, ToolContext, ToolOutput};
use ai_errors::{AiError, ToolError};
use ai_security::UrlPolicy;

/// Supported browser action kinds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum BrowserAction {
    /// Navigate to a target URL.
    Navigate { url: String },
    /// Click an element by CSS selector.
    Click { selector: String },
    /// Input text into a form element.
    TypeText { selector: String, text: String },
    /// Capture page screenshot.
    Screenshot {
        #[serde(default)]
        full_page: bool,
    },
}

/// A tool exposing browser action capabilities to models.
///
/// ⚠️ SIMULATED: acknowledges actions without driving any browser. For real
/// browser control use `ai_computer::omnichrome::BrowserTool` (OmniChrome
/// CDP bridge).
#[derive(Debug, Clone)]
#[deprecated(
    since = "0.1.0",
    note = "simulated acknowledgement only; use ai-computer's OmniChrome-backed BrowserTool for real control"
)]
pub struct BrowserTool {
    policy: UrlPolicy,
}

impl Default for BrowserTool {
    fn default() -> Self {
        Self::new()
    }
}

impl BrowserTool {
    pub fn new() -> Self {
        Self {
            policy: UrlPolicy::new(),
        }
    }

    pub fn with_url_policy(mut self, policy: UrlPolicy) -> Self {
        self.policy = policy;
        self
    }
}

#[async_trait::async_trait]
impl Tool for BrowserTool {
    fn name(&self) -> &str {
        "browser_action"
    }

    fn description(&self) -> &str {
        "Perform web browser actions (navigate, click, type text, screenshot) with SSRF policy validation."
    }

    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["navigate", "click", "type_text", "screenshot"],
                    "description": "The browser action to execute"
                },
                "url": { "type": "string", "description": "Target URL for navigate" },
                "selector": { "type": "string", "description": "CSS selector for click/type" },
                "text": { "type": "string", "description": "Text content to type" },
                "full_page": { "type": "boolean", "description": "Capture full page screenshot" }
            },
            "required": ["action"]
        })
    }

    async fn execute(
        &self,
        arguments: serde_json::Value,
        _context: &ToolContext,
    ) -> Result<ToolOutput, AiError> {
        let action: BrowserAction = serde_json::from_value(arguments).map_err(|e| {
            AiError::Tool(ToolError::new(
                "browser_action",
                format!("invalid action payload: {e}"),
            ))
        })?;

        match &action {
            BrowserAction::Navigate { url } => {
                self.policy.require(url)?;
                Ok(ToolOutput::ok(format!("navigated to `{url}` successfully")))
            }
            BrowserAction::Click { selector } => {
                if selector.trim().is_empty() {
                    return Err(AiError::Tool(ToolError::new(
                        "browser_action",
                        "selector cannot be empty",
                    )));
                }
                Ok(ToolOutput::ok(format!(
                    "clicked element `{selector}` successfully"
                )))
            }
            BrowserAction::TypeText { selector, text } => {
                if selector.trim().is_empty() {
                    return Err(AiError::Tool(ToolError::new(
                        "browser_action",
                        "selector cannot be empty",
                    )));
                }
                Ok(ToolOutput::ok(format!(
                    "typed text into `{selector}` successfully ({} chars)",
                    text.len()
                )))
            }
            BrowserAction::Screenshot { full_page } => Ok(ToolOutput::ok(format!(
                "captured screenshot (full_page={full_page}) successfully"
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn browser_tool_validates_ssrf_on_navigate() {
        let tool = BrowserTool::new();
        let ctx = ToolContext::default();
        let err = tool
            .execute(
                json!({"action":"navigate","url":"http://192.168.1.1/admin"}),
                &ctx,
            )
            .await
            .unwrap_err();
        assert!(err.to_string().contains("SSRF policy"));

        let ok = tool
            .execute(
                json!({"action":"navigate","url":"https://example.com"}),
                &ctx,
            )
            .await
            .unwrap();
        assert!(ok.content.contains("navigated to `https://example.com`"));
    }

    #[tokio::test]
    async fn browser_tool_executes_click_and_type() {
        let tool = BrowserTool::new();
        let ctx = ToolContext::default();
        let click_out = tool
            .execute(json!({"action":"click","selector":"#submit-btn"}), &ctx)
            .await
            .unwrap();
        assert!(click_out.content.contains("clicked element `#submit-btn`"));

        let type_out = tool
            .execute(
                json!({"action":"type_text","selector":"input#search","text":"hello"}),
                &ctx,
            )
            .await
            .unwrap();
        assert!(type_out.content.contains("5 chars"));
    }
}
