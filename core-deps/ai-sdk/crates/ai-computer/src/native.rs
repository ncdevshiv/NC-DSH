//! Native Computer Use desktop plugin — real OS-level automation via the
//! PowerShell engine's JSON-RPC server (`http://localhost:8888/rpc`).
//!
//! Wire contract (from protocol recon of `F:\computeruse-v2`):
//! - Bearer auth, exact-string `Authorization: Bearer <token>`; token from
//!   `COMPUTERUSE_TOKEN` env or `%USERPROFILE%\.computeruse\auth.token`.
//! - `type`/`paste`/`key` REQUIRE a `target` (hwnd | window/process
//!   substring | literal `"focused"`) unless the server runs with
//!   `COMPUTERUSE_REQUIRE_TARGET=0`; sentinel errors like
//!   `TARGET_REQUIRED: …` arrive as `-32000` bodies whose JSON-RPC `id`
//!   is always null.
//! - Result casing is inconsistent (`clickedAt` vs `Success`/`Bounds`) —
//!   read fields through [`crate::jsonrpc_client::field`].
//! - Screenshot base64 arrives as a full `data:image/png;base64,…` URL.
//!
//! Engine-down calls fail with typed, actionable errors — nothing is
//! fabricated. Start the engine with:
//! `powershell -File <computeruse>\server\NativeServer.ps1 -Port 8888`

use async_trait::async_trait;
use serde_json::{Value, json};

use crate::jsonrpc_client::{ComputerError, JsonRpcHttpClient, resolve_token};

/// Environment variable holding an explicit endpoint override.
pub const SERVER_URL_ENV: &str = "COMPUTERUSE_SERVER_URL";
/// Default loopback endpoint of the NativeServer.
pub const DEFAULT_ENDPOINT: &str = "http://localhost:8888/rpc";
/// Environment variable holding an explicit token.
pub const TOKEN_ENV: &str = "COMPUTERUSE_TOKEN";
/// Environment variable overriding the default token-file path.
pub const TOKEN_FILE_ENV: &str = "COMPUTERUSE_TOKEN_FILE";

/// Client for the Native Computer Use engine.
#[derive(Clone)]
pub struct NativeComputerClient {
    rpc: JsonRpcHttpClient,
    base_url: String,
    token: Option<String>,
    http: reqwest::Client,
}

impl NativeComputerClient {
    /// Builds a client from explicit values.
    pub fn new(endpoint: impl Into<String>, token: Option<String>) -> Self {
        let endpoint = endpoint.into();
        Self {
            rpc: JsonRpcHttpClient::new(endpoint.clone(), token.clone()),
            base_url: endpoint.trim_end_matches("/rpc").to_string(),
            token,
            http: reqwest::Client::new(),
        }
    }

    /// Builds a client from environment configuration (see module docs).
    pub fn with_env() -> Self {
        let endpoint =
            std::env::var(SERVER_URL_ENV).unwrap_or_else(|_| DEFAULT_ENDPOINT.to_string());
        let file = std::env::var(TOKEN_FILE_ENV)
            .map(std::path::PathBuf::from)
            .ok()
            .or_else(|| {
                std::env::var("USERPROFILE").ok().map(|home| {
                    std::path::PathBuf::from(home)
                        .join(".computeruse")
                        .join("auth.token")
                })
            });
        let token = resolve_token(None, TOKEN_ENV, file.as_deref());
        Self::new(endpoint, token)
    }

    fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Unauthenticated health probe: `{status:"online",engine:…}`.
    pub async fn health(&self) -> Result<Value, ComputerError> {
        let url = format!("{}/health", self.base_url());
        self.get_json(&url, false).await
    }

    /// Authenticated discovery: `{methods:[{name,description}],count}`.
    pub async fn methods(&self) -> Result<Value, ComputerError> {
        let url = format!("{}/methods", self.base_url());
        self.get_json(&url, true).await
    }

    /// Shared GET helper; `auth=false` only for `/health`.
    async fn get_json(&self, url: &str, auth: bool) -> Result<Value, ComputerError> {
        use ComputerError::EngineUnreachable;
        let mut req = self
            .http
            .get(url)
            .timeout(std::time::Duration::from_secs(15));
        if auth {
            if let Some(token) = &self.token {
                req = req.bearer_auth(token);
            }
        }
        let response = req
            .send()
            .await
            .map_err(|e| EngineUnreachable(format!("GET {url} failed: {e}")))?;
        let status = response.status();
        let body: Value = response
            .json()
            .await
            .map_err(|e| EngineUnreachable(format!("GET {url}: undecodable response ({e})")))?;
        if status.as_u16() == 401 {
            return Err(ComputerError::Unauthorized(
                "HTTP 401 from engine (bad or missing token)".into(),
            ));
        }
        if !status.is_success() {
            return Err(EngineUnreachable(format!("GET {url}: HTTP {status}")));
        }
        Ok(body)
    }

    // -- perception ---------------------------------------------------------

    pub async fn status(&self) -> Result<Value, ComputerError> {
        self.rpc.call("computer.status", json!({})).await
    }

    /// Captures the screen: server-side `path` when given, otherwise the
    /// decoded PNG bytes from the returned data-URL.
    pub async fn screenshot(
        &self,
        path: Option<&str>,
        full_desktop: bool,
    ) -> Result<ScreenshotResult, ComputerError> {
        let mut params = json!({ "fullDesktop": full_desktop });
        if let Some(p) = path {
            params["path"] = json!(p);
        }
        let result = self.rpc.call("computer.screenshot", params).await?;
        if let Some(saved) = crate::jsonrpc_client::field(&result, &["path"]) {
            return Ok(ScreenshotResult::Saved {
                path: saved.as_str().unwrap_or_default().to_string(),
            });
        }
        let b64 = crate::jsonrpc_client::field(&result, &["base64"])
            .and_then(|v| v.as_str())
            .ok_or_else(|| ComputerError::Rpc {
                code: -32000,
                message: "screenshot result had neither path nor base64".into(),
            })?;
        let bytes = base64_decode(strip_data_url(b64)).ok_or_else(|| ComputerError::Rpc {
            code: -32000,
            message: "invalid base64 screenshot".into(),
        })?;
        Ok(ScreenshotResult::Bytes(bytes))
    }

    pub async fn ocr_find(&self, pattern: &str) -> Result<Value, ComputerError> {
        self.rpc
            .call("computer.ocr_find", json!({ "pattern": pattern }))
            .await
    }

    pub async fn ocr_extract_all(&self) -> Result<Value, ComputerError> {
        self.rpc.call("computer.ocr_extract_all", json!({})).await
    }

    pub async fn ui_tree(&self, hwnd: i64) -> Result<Value, ComputerError> {
        self.rpc
            .call("computer.get_ui_tree", json!({ "hwnd": hwnd }))
            .await
    }

    pub async fn som(&self, hwnd: i64) -> Result<Value, ComputerError> {
        self.rpc
            .call("computer.show_som", json!({ "hwnd": hwnd }))
            .await
    }

    // -- input --------------------------------------------------------------

    pub async fn mouse_move(
        &self,
        x: i64,
        y: i64,
        duration_ms: u64,
    ) -> Result<Value, ComputerError> {
        self.rpc
            .call(
                "computer.mouse_move",
                json!({ "x": x, "y": y, "durationMs": duration_ms }),
            )
            .await
    }

    pub async fn click(
        &self,
        x: i64,
        y: i64,
        button: Option<&str>,
        click_count: u32,
    ) -> Result<Value, ComputerError> {
        self.rpc
            .call(
                "computer.click",
                json!({ "x": x, "y": y, "button": button, "clickCount": click_count }),
            )
            .await
    }

    pub async fn double_click(&self, x: i64, y: i64) -> Result<Value, ComputerError> {
        self.rpc
            .call("computer.double_click", json!({ "x": x, "y": y }))
            .await
    }

    pub async fn right_click(&self, x: i64, y: i64) -> Result<Value, ComputerError> {
        self.rpc
            .call("computer.right_click", json!({ "x": x, "y": y }))
            .await
    }

    pub async fn drag(
        &self,
        start_x: i64,
        start_y: i64,
        end_x: i64,
        end_y: i64,
        duration_ms: u64,
    ) -> Result<Value, ComputerError> {
        self.rpc.call("computer.drag_and_drop", json!({
            "startX": start_x, "startY": start_y, "endX": end_x, "endY": end_y, "durationMs": duration_ms
        })).await
    }

    pub async fn scroll(&self, clicks: i64, horizontal: bool) -> Result<Value, ComputerError> {
        self.rpc
            .call(
                "computer.scroll",
                json!({ "clicks": clicks, "horizontal": horizontal }),
            )
            .await
    }

    pub async fn type_text(&self, text: &str, target: &str) -> Result<Value, ComputerError> {
        require_target(target)?;
        self.rpc
            .call("computer.type", json!({ "text": text, "target": target }))
            .await
    }

    pub async fn paste(&self, text: &str, target: &str) -> Result<Value, ComputerError> {
        require_target(target)?;
        self.rpc
            .call("computer.paste", json!({ "text": text, "target": target }))
            .await
    }

    pub async fn key(
        &self,
        key: &str,
        modifiers: &[&str],
        target: &str,
    ) -> Result<Value, ComputerError> {
        require_target(target)?;
        self.rpc
            .call(
                "computer.key",
                json!({ "key": key, "modifiers": modifiers, "target": target }),
            )
            .await
    }

    pub async fn clipboard_get(&self) -> Result<Value, ComputerError> {
        self.rpc.call("computer.clipboard_get", json!({})).await
    }

    pub async fn clipboard_set(&self, text: &str) -> Result<Value, ComputerError> {
        self.rpc
            .call("computer.clipboard_set", json!({ "text": text }))
            .await
    }

    // -- windows / waits ----------------------------------------------------

    pub async fn focus_window(&self, pattern: &str) -> Result<Value, ComputerError> {
        self.rpc
            .call("computer.focus_window", json!({ "pattern": pattern }))
            .await
    }

    pub async fn list_windows(&self) -> Result<Value, ComputerError> {
        self.rpc.call("computer.list_windows", json!({})).await
    }

    pub async fn wait_change(&self, timeout_ms: u64) -> Result<Value, ComputerError> {
        self.rpc
            .call("computer.wait_change", json!({ "timeoutMs": timeout_ms }))
            .await
    }

    pub async fn wait_text(&self, pattern: &str, timeout_ms: u64) -> Result<Value, ComputerError> {
        self.rpc
            .call(
                "computer.wait_text",
                json!({ "pattern": pattern, "timeoutMs": timeout_ms }),
            )
            .await
    }

    // -- shell / misc ---------------------------------------------------------

    pub async fn shell_open(&self, target: &str) -> Result<Value, ComputerError> {
        self.rpc
            .call("computer.shell_open", json!({ "target": target }))
            .await
    }

    /// System telemetry: `{TotalMemoryMB,FreeMemoryMB,…}`.
    pub async fn telemetry(&self) -> Result<Value, ComputerError> {
        self.rpc.call("computer.telemetry", json!({})).await
    }
}

/// Outcome of [`NativeComputerClient::screenshot`].
#[derive(Debug, Clone)]
pub enum ScreenshotResult {
    /// Server wrote the file itself.
    Saved { path: String },
    /// Decoded PNG/JPEG bytes (data-URL prefix stripped).
    Bytes(Vec<u8>),
}

fn require_target(target: &str) -> Result<(), ComputerError> {
    if target.trim().is_empty() {
        Err(ComputerError::InvalidArgs(
            "`target` is required by the engine for keyboard/typing actions \
             (hwnd, window/process substring, or \"focused\")"
                .into(),
        ))
    } else {
        Ok(())
    }
}

/// Strips a leading `data:<mime>;base64,` prefix when present.
fn strip_data_url(s: &str) -> &str {
    match s.find("base64,") {
        Some(idx) => &s[idx + "base64,".len()..],
        None => s,
    }
}

/// Standard-alphabet base64 decoder (no external dependencies).
pub fn base64_decode(input: &str) -> Option<Vec<u8>> {
    fn val(c: u8) -> Option<u32> {
        match c {
            b'A'..=b'Z' => Some((c - b'A') as u32),
            b'a'..=b'z' => Some((c - b'a') as u32 + 26),
            b'0'..=b'9' => Some((c - b'0') as u32 + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }
    let cleaned: Vec<u8> = input
        .bytes()
        .filter(|b| !b.is_ascii_whitespace() && *b != b'=')
        .collect();
    let mut out = Vec::with_capacity(cleaned.len() * 3 / 4);
    for chunk in cleaned.chunks(4) {
        if chunk.len() == 1 {
            return None; // dangling 6 bits cannot form a byte
        }
        let mut acc: u32 = 0;
        for (i, &c) in chunk.iter().enumerate() {
            acc |= val(c)? << (18 - 6 * i);
        }
        out.push((acc >> 16) as u8);
        if chunk.len() > 2 {
            out.push((acc >> 8) as u8);
        }
        if chunk.len() > 3 {
            out.push(acc as u8);
        }
    }
    Some(out)
}

// ---------------------------------------------------------------------------
// Tool
// ---------------------------------------------------------------------------

/// Real desktop-control tool backed by the Native Computer Use engine.
///
/// Single action-dispatch surface so agents see one tool; every action maps
/// to a documented `computer.*` RPC. Requires the engine to be running —
/// engine-down failures are typed errors, never fabricated success.
pub struct ComputerTool {
    client: NativeComputerClient,
}

impl ComputerTool {
    pub fn new(client: NativeComputerClient) -> Self {
        Self { client }
    }

    /// Builds a client from environment configuration.
    pub fn from_env() -> Self {
        Self::new(NativeComputerClient::with_env())
    }
}

#[async_trait]
impl ai_tools::Tool for ComputerTool {
    fn name(&self) -> &str {
        "computer"
    }

    fn description(&self) -> &str {
        "Control the local Windows desktop via the Native Computer Use \
         engine: screenshots, OCR text-finding, Set-of-Marks UI tree, \
         human-like mouse/keyboard, window management, visual waits. \
         Requires the computeruse NativeServer on localhost:8888."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": [
                        "status", "screenshot", "click", "double_click",
                        "right_click", "mouse_move", "drag", "scroll", "type",
                        "paste", "key", "clipboard_get", "clipboard_set",
                        "ocr_find", "som", "wait_change", "focus_window",
                        "list_windows", "shell_open", "telemetry", "methods"
                    ],
                    "description": "Desktop operation to perform"
                },
                "params": {
                    "type": "object",
                    "description": "Method parameters (verbatim engine names: x/y/clickCount/durationMs/pattern/target/timeoutMs/...)"
                },
                "save_path": {
                    "type": "string",
                    "description": "(screenshot) server-side output path; omit to receive base64 bytes"
                }
            },
            "required": ["action"]
        })
    }

    fn required_permissions(&self) -> Vec<&str> {
        vec!["net:http", "desktop:control"]
    }

    async fn execute(
        &self,
        arguments: Value,
        _context: &ai_tools::ToolContext,
    ) -> Result<ai_tools::ToolOutput, ai_errors::AiError> {
        use ComputerError::InvalidArgs;

        let action = arguments
            .get("action")
            .and_then(|a| a.as_str())
            .ok_or_else(|| ComputerError::InvalidArgs("`action` is required".into()))?;
        let params = arguments.get("params").cloned().unwrap_or(json!({}));
        if !params.is_object() {
            return Err(ComputerError::InvalidArgs("`params` must be an object".into()).into());
        }

        let value: Value = match action {
            "status" => self.client.status().await?,
            "screenshot" => {
                let save_path = arguments.get("save_path").and_then(|v| v.as_str());
                let full_desktop = params
                    .get("fullDesktop")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true);
                match self.client.screenshot(save_path, full_desktop).await? {
                    ScreenshotResult::Saved { path } => json!({ "success": true, "savedTo": path }),
                    ScreenshotResult::Bytes(bytes) => {
                        let mut out = json!({
                            "success": true,
                            "byteLen": bytes.len(),
                            "pngMagic": bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
                        });
                        // Short preview only: full base64 would dwarf context.
                        let preview: String =
                            bytes.iter().take(40).map(|b| format!("{b:02x}")).collect();
                        out["bytesPreview"] = Value::String(preview);
                        out
                    }
                }
            }
            "click" => {
                let (x, y) = xy(&params)?;
                self.client
                    .click(
                        x,
                        y,
                        str_param(&params, "button"),
                        u32_param(&params, "clickCount").unwrap_or(1),
                    )
                    .await?
            }
            "double_click" => {
                let (x, y) = xy(&params)?;
                self.client.double_click(x, y).await?
            }
            "right_click" => {
                let (x, y) = xy(&params)?;
                self.client.right_click(x, y).await?
            }
            "mouse_move" => {
                let (x, y) = xy(&params)?;
                self.client
                    .mouse_move(x, y, u64_param(&params, "durationMs").unwrap_or(200))
                    .await?
            }
            "drag" => {
                self.client
                    .drag(
                        int_param(&params, "startX")
                            .ok_or_else(|| InvalidArgs("startX required".into()))?,
                        int_param(&params, "startY")
                            .ok_or_else(|| InvalidArgs("startY required".into()))?,
                        int_param(&params, "endX")
                            .ok_or_else(|| InvalidArgs("endX required".into()))?,
                        int_param(&params, "endY")
                            .ok_or_else(|| InvalidArgs("endY required".into()))?,
                        u64_param(&params, "durationMs").unwrap_or(500),
                    )
                    .await?
            }
            "scroll" => {
                let clicks = int_param(&params, "clicks")
                    .ok_or_else(|| InvalidArgs("clicks required (negative = down)".into()))?;
                self.client
                    .scroll(clicks, bool_param(&params, "horizontal"))
                    .await?
            }
            "type" => {
                let text = str_req(&params, "text")?;
                let target = str_req(&params, "target")?;
                self.client.type_text(text, target).await?
            }
            "paste" => {
                let text = str_req(&params, "text")?;
                let target = str_req(&params, "target")?;
                self.client.paste(text, target).await?
            }
            "key" => {
                let key = str_req(&params, "key")?;
                let target = str_req(&params, "target")?;
                let modifiers: Vec<&str> = params
                    .get("modifiers")
                    .and_then(|m| m.as_array())
                    .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
                    .unwrap_or_default();
                self.client.key(key, &modifiers, target).await?
            }
            "clipboard_get" => self.client.clipboard_get().await?,
            "clipboard_set" => self.client.clipboard_set(str_req(&params, "text")?).await?,
            "ocr_find" => self.client.ocr_find(str_req(&params, "pattern")?).await?,
            "som" => {
                self.client
                    .som(int_param(&params, "hwnd").unwrap_or(0))
                    .await?
            }
            "wait_change" => {
                self.client
                    .wait_change(u64_param(&params, "timeoutMs").unwrap_or(5_000))
                    .await?
            }
            "focus_window" => {
                self.client
                    .focus_window(str_req(&params, "pattern")?)
                    .await?
            }
            "list_windows" => self.client.list_windows().await?,
            "shell_open" => self.client.shell_open(str_req(&params, "target")?).await?,
            "telemetry" => self.rpc_telemetry().await,
            "methods" => serde_json::to_value(self.client.methods().await?).unwrap_or(Value::Null),
            unknown => {
                return Err(
                    InvalidArgs(format!("unknown action `{unknown}`; see input_schema")).into(),
                );
            }
        };

        Ok(ai_tools::ToolOutput::ok(compact(&value)))
    }
}

impl ComputerTool {
    async fn rpc_telemetry(&self) -> Value {
        self.client
            .telemetry()
            .await
            .unwrap_or_else(|e| json!({ "error": e.to_string() }))
    }
}

// -- argument helpers -------------------------------------------------------

fn xy(params: &Value) -> Result<(i64, i64), ComputerError> {
    let x =
        int_param(params, "x").ok_or_else(|| ComputerError::InvalidArgs("x required".into()))?;
    let y =
        int_param(params, "y").ok_or_else(|| ComputerError::InvalidArgs("y required".into()))?;
    Ok((x, y))
}

fn int_param(params: &Value, name: &str) -> Option<i64> {
    params.get(name).and_then(|v| v.as_i64())
}

fn u64_param(params: &Value, name: &str) -> Option<u64> {
    params.get(name).and_then(|v| v.as_u64())
}

fn u32_param(params: &Value, name: &str) -> Option<u32> {
    params.get(name).and_then(|v| v.as_u64()).map(|v| v as u32)
}

fn bool_param(params: &Value, name: &str) -> bool {
    params.get(name).and_then(|v| v.as_bool()).unwrap_or(false)
}

fn str_param<'a>(params: &'a Value, name: &str) -> Option<&'a str> {
    params.get(name).and_then(|v| v.as_str())
}

fn str_req<'a>(params: &'a Value, name: &str) -> Result<&'a str, ComputerError> {
    str_param(params, name)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ComputerError::InvalidArgs(format!("`{name}` is required")))
}
/// Compact single-line JSON with base64-looking strings truncated.
fn compact(v: &Value) -> String {
    let s = v.to_string();
    if s.len() <= 2_000 {
        return s;
    }
    format!(
        "{}…",
        &s[..s
            .char_indices()
            .nth(2_000)
            .map(|(i, _)| i)
            .unwrap_or(s.len())]
    )
}
