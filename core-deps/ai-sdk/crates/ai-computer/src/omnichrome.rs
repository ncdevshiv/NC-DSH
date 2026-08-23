//! Real browser automation against the OmniChrome Chrome-extension bridge.
//!
//! This module is the honest, typed replacement for simulated browser
//! affordances: an [`OmniChromeClient`] speaking the bridge's authenticated
//! JSON-RPC 2.0 surface, plus a drop-in [`BrowserTool`] implementing
//! [`ai_tools::Tool`]. When the bridge or its extension is down, every call
//! fails with a typed, actionable error — nothing is fabricated.
//!
//! # Wire contract (protocol recon — verbatim casing)
//!
//! - Endpoint: `POST {bridge}/rpc`, default `http://localhost:8765/rpc`
//!   (override with `OMNICHROME_BRIDGE_URL`).
//! - Auth: `Authorization: Bearer <token>`; token from `OMNICHROME_TOKEN`,
//!   else the file named by `OMNICHROME_TOKEN_FILE`, else
//!   `<OMNICHROME_HOME>/server/.bridge-token` (no default root). Missing
//!   everywhere ⇒ no auth header; the bridge then rejects with 401.
//! - HTTP 401 wraps JSON-RPC `-32001` ⇒ [`ComputerError::Unauthorized`].
//! - Forwarding failures surface as HTTP 500 wrapping `-32000` (extension
//!   timeout or "no extension attached"). The seam maps those to
//!   [`ComputerError::Rpc`]; this plugin re-surfaces them as
//!   [`ComputerError::EngineUnreachable`] with a remediation hint, because
//!   for OmniChrome `-32000` *means* "cannot reach the extension".
//! - Engine-down signature:
//!   `No active Chrome Extension connected to bridge…`.
//!
//! # Scope notes (v1)
//!
//! - `agent.runTask` returns only the immediate `{status:"started", …}`
//!   acknowledgement. Progress events are delivered over WebSocket and are
//!   **out of scope** for v1 — poll side effects via tabs/logs instead.
//! - `browser.click` with neither `{x,y}` nor `{selector}` makes the bridge
//!   hang for ~30 s. [`OmniChromeClient::click_xy`],
//!   [`click_selector`](OmniChromeClient::click_selector) and the tool all
//!   validate client-side and return [`ComputerError::InvalidArgs`] before
//!   any network traffic.
//! - [`BrowserTool::required_permissions`] always includes `"fs:write"`.
//!   The [`ai_tools::Tool`] contract exposes permissions statically, so a
//!   save_path-only capability cannot be declared per-call; requiring
//!   `fs:write` up front is the conservative choice (the gate is checked
//!   before any execution) at the cost of denying non-screenshot actions
//!   to contexts without filesystem grants. Documented trade-off, deliberate.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::{Map, Value, json};

use crate::jsonrpc_client::{ComputerError, JsonRpcHttpClient, field, resolve_token};
use ai_errors::{AiError, ToolError};
use ai_tools::{Tool, ToolContext, ToolOutput};

/// Default OmniChrome bridge RPC endpoint.
pub const DEFAULT_BRIDGE_URL: &str = "http://localhost:8765/rpc";

const ENV_URL: &str = "OMNICHROME_BRIDGE_URL";
const ENV_TOKEN: &str = "OMNICHROME_TOKEN";
const ENV_TOKEN_FILE: &str = "OMNICHROME_TOKEN_FILE";
const ENV_HOME: &str = "OMNICHROME_HOME";

/// Remediation hint appended whenever the bridge looks unreachable.
const ENGINE_DOWN_HINT: &str = "start the OmniChrome bridge (`node server/bridge-server.js` \
in the OmniChrome checkout) and load/enable the Chrome extension, then retry";

const TOOL_NAME: &str = "browser_action";
/// Retained leading characters of a data URL / oversized string in content.
const DATA_URL_PREVIEW_CHARS: usize = 72;
/// Hard cap for any single string embedded in tool content.
const MAX_INLINE_STRING: usize = 4000;

// ---------------------------------------------------------------------------
// Result types (lenient: absent engine fields stay `None`/null — no guessing)
// ---------------------------------------------------------------------------

/// Response of `GET /health`.
#[derive(Debug, Clone)]
pub struct HealthInfo {
    /// Engine-reported status string, when present.
    pub status: Option<String>,
    /// The full health payload.
    pub raw: Value,
}

/// The bridge's currently focused tab (`browser.getStatus`).
#[derive(Debug, Clone)]
pub struct ActiveTab {
    pub id: Value,
    pub url: Option<String>,
    pub title: Option<String>,
}

/// Aggregated connection state (`browser.getStatus`).
#[derive(Debug, Clone)]
pub struct StatusInfo {
    pub status: Option<String>,
    pub connected_tabs: Option<u64>,
    pub active_tab: Option<ActiveTab>,
    pub raw: Value,
}

/// One open tab (`browser.getTabs`).
#[derive(Debug, Clone)]
pub struct TabInfo {
    pub id: Value,
    pub url: Option<String>,
    pub title: Option<String>,
    pub active: Option<bool>,
    pub muted: Option<bool>,
}

/// A freshly opened tab (`browser.createTab` → `{id,url}`).
#[derive(Debug, Clone)]
pub struct CreatedTab {
    pub id: Value,
    pub url: Option<String>,
}

/// Navigation outcome (`browser.navigate` → `{success,url,tabId}`).
#[derive(Debug, Clone)]
pub struct NavigationResult {
    pub success: Option<bool>,
    pub url: Option<String>,
    pub tab_id: Option<Value>,
}

/// Click outcome. Coordinate clicks fill `x`/`y`; selector clicks fill
/// `clicked_at` from `clickedAt{x,y}`.
#[derive(Debug, Clone, Default)]
pub struct ClickResult {
    pub success: Option<bool>,
    pub x: Option<f64>,
    pub y: Option<f64>,
    pub clicked_at: Option<(f64, f64)>,
}

/// Immediate ack of `agent.runTask` (`{status:"started",goal,tabId}`).
/// Task progress itself arrives over WebSocket and is out of scope in v1.
#[derive(Debug, Clone)]
pub struct RunTaskStarted {
    pub status: Option<String>,
    pub goal: Option<String>,
    pub tab_id: Option<Value>,
}

/// Raw screenshot payload: the engine's data URL plus decoded bytes.
#[derive(Debug, Clone)]
pub struct ScreenshotDataUrl {
    /// e.g. `data:image/png;base64,…` (verbatim from the engine).
    pub data_url: String,
    /// Bytes decoded from the base64 section.
    pub bytes: Vec<u8>,
}

fn opt_str(v: &Value, names: &[&str]) -> Option<String> {
    field(v, names).and_then(Value::as_str).map(str::to_owned)
}

fn opt_bool(v: &Value, names: &[&str]) -> Option<bool> {
    field(v, names).and_then(Value::as_bool)
}

fn opt_f64_named(v: &Value, names: &[&str]) -> Option<f64> {
    field(v, names).and_then(Value::as_f64)
}

impl HealthInfo {
    fn parse(raw: Value) -> Self {
        Self {
            status: opt_str(&raw, &["status"]),
            raw,
        }
    }
}

impl StatusInfo {
    fn parse(raw: Value) -> Self {
        let active_tab = field(&raw, &["activeTab"])
            .filter(|t| !t.is_null())
            .map(|t| ActiveTab {
                id: t.get("id").cloned().unwrap_or(Value::Null),
                url: opt_str(t, &["url"]),
                title: opt_str(t, &["title"]),
            });
        Self {
            status: opt_str(&raw, &["status"]),
            connected_tabs: field(&raw, &["connectedTabs"]).and_then(Value::as_u64),
            active_tab,
            raw,
        }
    }
}

impl TabInfo {
    fn parse(v: &Value) -> Self {
        Self {
            id: v.get("id").cloned().unwrap_or(Value::Null),
            url: opt_str(v, &["url"]),
            title: opt_str(v, &["title"]),
            active: opt_bool(v, &["active"]),
            muted: opt_bool(v, &["muted"]),
        }
    }
}

impl CreatedTab {
    fn parse(raw: Value) -> Self {
        Self {
            id: raw.get("id").cloned().unwrap_or(Value::Null),
            url: opt_str(&raw, &["url"]),
        }
    }
}

impl NavigationResult {
    fn parse(raw: Value) -> Self {
        Self {
            success: opt_bool(&raw, &["success"]),
            url: opt_str(&raw, &["url"]),
            tab_id: raw.get("tabId").cloned(),
        }
    }
}

impl ClickResult {
    fn parse(raw: Value) -> Self {
        let clicked_at = field(&raw, &["clickedAt"])
            .and_then(|c| Some((opt_f64_named(c, &["x"])?, opt_f64_named(c, &["y"])?)));
        Self {
            success: opt_bool(&raw, &["success"]),
            x: opt_f64_named(&raw, &["x"]),
            y: opt_f64_named(&raw, &["y"]),
            clicked_at,
        }
    }
}

impl RunTaskStarted {
    fn parse(raw: Value) -> Self {
        Self {
            status: opt_str(&raw, &["status"]),
            goal: opt_str(&raw, &["goal"]),
            tab_id: raw.get("tabId").cloned(),
        }
    }
}

// ---------------------------------------------------------------------------
// Small pure helpers
// ---------------------------------------------------------------------------

/// True for blank/whitespace-only strings.
fn blank(s: &str) -> bool {
    s.trim().is_empty()
}

/// Maps the shared transport errors onto OmniChrome-specific semantics:
/// `-32000` body errors are forwarding failures ("extension not attached"
/// / timeout), so they become actionable [`ComputerError::EngineUnreachable`]
/// values carrying [`ENGINE_DOWN_HINT`].
fn surface_engine_down(e: ComputerError) -> ComputerError {
    match e {
        ComputerError::Rpc {
            code: -32000,
            message,
        } => ComputerError::EngineUnreachable(format!(
            "OmniChrome bridge cannot forward to the extension: {message}; {ENGINE_DOWN_HINT}"
        )),
        ComputerError::EngineUnreachable(message) => {
            ComputerError::EngineUnreachable(format!("{message}; {ENGINE_DOWN_HINT}"))
        }
        other => other,
    }
}

/// Derives the health URL from the RPC endpoint: strips a trailing `/rpc`
/// and appends `/health`; endpoints without that suffix gain `/health`
/// directly.
fn health_url(endpoint: &str) -> String {
    let base = endpoint.strip_suffix("/rpc").unwrap_or(endpoint);
    format!("{base}/health")
}

/// Resolves the token-file candidate from a key→value lookup:
/// `OMNICHROME_TOKEN_FILE` wins, else `<OMNICHROME_HOME>/server/.bridge-token`.
fn token_file_path(read: impl Fn(&str) -> Option<String>) -> Option<PathBuf> {
    if let Some(f) = read(ENV_TOKEN_FILE).filter(|v| !blank(v)) {
        return Some(PathBuf::from(f.trim()));
    }
    read(ENV_HOME).filter(|v| !blank(v)).map(|root| {
        PathBuf::from(root.trim())
            .join("server")
            .join(".bridge-token")
    })
}

/// Resolves the RPC endpoint from a lookup: env override or the default.
fn endpoint_from(read: impl Fn(&str) -> Option<String>) -> String {
    read(ENV_URL)
        .filter(|v| !blank(v))
        .map(|v| v.trim().to_string())
        .unwrap_or_else(|| DEFAULT_BRIDGE_URL.to_string())
}

/// Standard-alphabet base64 decoder (whitespace-tolerant, padding-aware).
/// Hand-rolled because this crate's dependency set is frozen (no `base64`).
pub(crate) fn base64_decode(input: &str) -> Option<Vec<u8>> {
    fn symbol(b: u8) -> Option<u32> {
        match b {
            b'A'..=b'Z' => Some((b - b'A') as u32),
            b'a'..=b'z' => Some((b - b'a' + 26) as u32),
            b'0'..=b'9' => Some((b - b'0' + 52) as u32),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }
    let mut out = Vec::with_capacity(input.len() / 4 * 3 + 3);
    let mut acc: u32 = 0;
    let mut bits: u32 = 0;
    let mut padding = false;
    for b in input.bytes().filter(|b| !b.is_ascii_whitespace()) {
        let v = if b == b'=' {
            padding = true;
            continue;
        } else {
            if padding {
                // Data after the first '=' — malformed.
                return None;
            }
            symbol(b)?
        };
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((acc >> bits) & 0xFF) as u8);
        }
    }
    // Only 0, 2 or 4 leftover bits are possible for well-formed input
    // (empty input legitimately decodes to zero bytes).
    if bits >= 6 {
        return None;
    }
    Some(out)
}

/// Splits a `data:<mime>;base64,<payload>` URL and decodes the payload.
/// Lenient fallback: strings without a comma are treated as bare base64.
fn decode_data_url(data_url: &str) -> Result<Vec<u8>, ComputerError> {
    let payload = match data_url.split_once(',') {
        Some((_, rest)) => rest,
        None => data_url,
    };
    base64_decode(payload).ok_or_else(|| ComputerError::Rpc {
        code: -32603,
        message: "screenshot: engine returned a dataUrl whose base64 payload does not decode"
            .to_string(),
    })
}

/// Keeps at most `keep` characters of `s`, appending a short elision marker
/// so total length stays bounded (≤ keep + ~12 chars).
fn truncate_chars(s: &str, keep: usize) -> String {
    if s.chars().count() <= keep {
        return s.to_string();
    }
    let kept: String = s.chars().take(keep).collect();
    let total = s.chars().count();
    format!("{kept}…+{total}")
}

/// Recursively bounds strings inside tool content: data URLs are cut to
/// [`DATA_URL_PREVIEW_CHARS`] and any other string to [`MAX_INLINE_STRING`].
fn sanitize_for_content(v: &Value) -> Value {
    match v {
        Value::String(s) => {
            if s.starts_with("data:") {
                Value::String(truncate_chars(s, DATA_URL_PREVIEW_CHARS))
            } else if s.chars().count() > MAX_INLINE_STRING {
                Value::String(truncate_chars(s, MAX_INLINE_STRING))
            } else {
                Value::String(s.clone())
            }
        }
        Value::Array(items) => Value::Array(items.iter().map(sanitize_for_content).collect()),
        Value::Object(map) => {
            let out: Map<String, Value> = map
                .iter()
                .map(|(k, val)| (k.clone(), sanitize_for_content(val)))
                .collect();
            Value::Object(out)
        }
        other => other.clone(),
    }
}

fn compact_json(v: &Value) -> String {
    serde_json::to_string(&sanitize_for_content(v)).unwrap_or_else(|_| "{}".to_string())
}

// ---------------------------------------------------------------------------
// Client
// ---------------------------------------------------------------------------

/// Typed client for the OmniChrome bridge. Cheap to clone conceptually —
/// wrap in [`Arc`] for sharing across tools.
pub struct OmniChromeClient {
    endpoint: String,
    /// Kept for `with_timeout` rebuilds; never logged or serialized.
    token: Option<String>,
    http: JsonRpcHttpClient,
    raw_http: reqwest::Client,
    timeout: Duration,
}

impl std::fmt::Debug for OmniChromeClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OmniChromeClient")
            .field("endpoint", &self.endpoint)
            .field("token", &self.token.as_ref().map(|_| "***redacted***"))
            .field("timeout", &self.timeout)
            .finish_non_exhaustive()
    }
}

impl OmniChromeClient {
    /// Explicit constructor. `endpoint` should be the RPC endpoint
    /// (`…/rpc`); pass `None` token to go unauthenticated (the bridge will
    /// reject unless anonymous access is enabled).
    pub fn new(endpoint: impl Into<String>, token: Option<String>) -> Self {
        Self::build(endpoint.into(), token, Duration::from_secs(35))
    }

    /// Environment-driven constructor: `OMNICHROME_BRIDGE_URL` (else
    /// [`DEFAULT_BRIDGE_URL`]) plus token resolution via
    /// [`resolve_token`] (`OMNICHROME_TOKEN` → `OMNICHROME_TOKEN_FILE` →
    /// `<OMNICHROME_HOME>/server/.bridge-token`).
    pub fn with_env() -> Self {
        let endpoint = endpoint_from(|k| std::env::var(k).ok());
        let file = token_file_path(|k| std::env::var(k).ok());
        let token = resolve_token(None, ENV_TOKEN, file.as_deref());
        Self::new(endpoint, token)
    }

    fn build(endpoint: String, token: Option<String>, timeout: Duration) -> Self {
        let http = JsonRpcHttpClient::new(endpoint.clone(), token.clone()).with_timeout(timeout);
        Self {
            endpoint,
            token,
            http,
            raw_http: reqwest::Client::new(),
            timeout,
        }
    }

    /// Overrides the per-call timeout (must exceed the bridge's own 30 s
    /// forwarding cap so *its* timeout message surfaces first).
    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self.http =
            JsonRpcHttpClient::new(self.endpoint.clone(), self.token.clone()).with_timeout(timeout);
        self
    }

    /// The configured RPC endpoint.
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    /// One authenticated RPC round-trip with OmniChrome-specific error
    /// surfacing (see [`surface_engine_down`]).
    async fn rpc(&self, method: &str, params: Value) -> Result<Value, ComputerError> {
        self.http
            .call(method, params)
            .await
            .map_err(surface_engine_down)
    }

    /// `GET /health` straight over HTTP (not JSON-RPC), bearer-authenticated
    /// when a token is configured.
    pub async fn health(&self) -> Result<HealthInfo, ComputerError> {
        let url = health_url(&self.endpoint);
        let mut req = self.raw_http.get(&url).timeout(self.timeout);
        if let Some(token) = &self.token {
            req = req.bearer_auth(token);
        }
        let response = req.send().await.map_err(|e| {
            if e.is_timeout() {
                ComputerError::Timeout(format!("GET {url} exceeded {:?}", self.timeout))
            } else {
                surface_engine_down(ComputerError::EngineUnreachable(format!(
                    "health: cannot reach {url} ({e})"
                )))
            }
        })?;
        let status = response.status();
        if status.as_u16() == 401 {
            return Err(ComputerError::Unauthorized(
                "GET /health returned HTTP 401 (bad or missing token)".into(),
            ));
        }
        if !status.is_success() {
            return Err(surface_engine_down(ComputerError::EngineUnreachable(
                format!("health: bridge returned HTTP {status}"),
            )));
        }
        let raw: Value = response.json().await.map_err(|e| {
            ComputerError::EngineUnreachable(format!("health: undecodable response ({e})"))
        })?;
        Ok(HealthInfo::parse(raw))
    }

    /// `browser.getStatus` → connection state + active tab.
    pub async fn status(&self) -> Result<StatusInfo, ComputerError> {
        Ok(StatusInfo::parse(
            self.rpc("browser.getStatus", json!({})).await?,
        ))
    }

    /// `browser.getTabs` → every open tab.
    pub async fn tabs(&self) -> Result<Vec<TabInfo>, ComputerError> {
        let res = self.rpc("browser.getTabs", json!({})).await?;
        Ok(res
            .as_array()
            .map(|a| a.iter().map(TabInfo::parse).collect())
            .unwrap_or_default())
    }

    /// `browser.createTab` — opens `url` (when given), optionally focused.
    pub async fn create_tab(
        &self,
        url: Option<&str>,
        active: Option<bool>,
    ) -> Result<CreatedTab, ComputerError> {
        if url.is_some_and(blank) {
            return Err(ComputerError::InvalidArgs(
                "create_tab: url must not be empty".into(),
            ));
        }
        let mut params = Map::new();
        if let Some(u) = url {
            params.insert("url".into(), json!(u));
        }
        if let Some(a) = active {
            params.insert("active".into(), json!(a));
        }
        Ok(CreatedTab::parse(
            self.rpc("browser.createTab", Value::Object(params)).await?,
        ))
    }

    /// `browser.switchTab {tabId}` — tab ids are opaque engine values
    /// (number or string), passed through verbatim.
    pub async fn switch_tab(&self, tab_id: Value) -> Result<Value, ComputerError> {
        self.validate_tab_id(&tab_id, "switch_tab")?;
        self.rpc("browser.switchTab", json!({ "tabId": tab_id }))
            .await
    }

    /// `browser.closeTab {tabId}`.
    pub async fn close_tab(&self, tab_id: Value) -> Result<Value, ComputerError> {
        self.validate_tab_id(&tab_id, "close_tab")?;
        self.rpc("browser.closeTab", json!({ "tabId": tab_id }))
            .await
    }

    /// `browser.navigate {url}`.
    pub async fn navigate(&self, url: &str) -> Result<NavigationResult, ComputerError> {
        if blank(url) {
            return Err(ComputerError::InvalidArgs(
                "navigate: url must not be empty".into(),
            ));
        }
        let res = self
            .rpc("browser.navigate", json!({ "url": url.trim() }))
            .await?;
        Ok(NavigationResult::parse(res))
    }

    /// `browser.click {x,y[,tabId]}` — validated client-side: coordinates
    /// must be finite numbers, otherwise [`ComputerError::InvalidArgs`]
    /// **before any network traffic** (the bridge hangs ~30 s on an empty
    /// click target).
    pub async fn click_xy(
        &self,
        x: f64,
        y: f64,
        tab_id: Option<Value>,
    ) -> Result<ClickResult, ComputerError> {
        if !x.is_finite() || !y.is_finite() {
            return Err(ComputerError::InvalidArgs(
                "click_xy: x and y must be finite numbers".into(),
            ));
        }
        let mut params = json!({ "x": x, "y": y });
        if let Some(t) = tab_id {
            self.validate_tab_id(&t, "click_xy")?;
            params["tabId"] = t;
        }
        Ok(ClickResult::parse(self.rpc("browser.click", params).await?))
    }

    /// `browser.click {selector[,tabId]}` — same client-side validation
    /// contract as [`Self::click_xy`]: a blank selector never hits the wire.
    pub async fn click_selector(
        &self,
        selector: &str,
        tab_id: Option<Value>,
    ) -> Result<ClickResult, ComputerError> {
        if blank(selector) {
            return Err(ComputerError::InvalidArgs(
                "click_selector: selector must not be empty".into(),
            ));
        }
        let mut params = json!({ "selector": selector.trim() });
        if let Some(t) = tab_id {
            self.validate_tab_id(&t, "click_selector")?;
            params["tabId"] = t;
        }
        Ok(ClickResult::parse(self.rpc("browser.click", params).await?))
    }

    /// `browser.type {text[,selector,submit,clearFirst][,tabId]}`.
    /// Client-side validation: `text` must be non-empty (after trim);
    /// `submit`/`clear_first` are forwarded verbatim.
    pub async fn type_text(
        &self,
        text: &str,
        selector: Option<&str>,
        submit: bool,
        clear_first: bool,
        tab_id: Option<Value>,
    ) -> Result<Value, ComputerError> {
        if blank(text) {
            return Err(ComputerError::InvalidArgs(
                "type_text: text must not be empty".into(),
            ));
        }
        if selector.is_some_and(blank) {
            return Err(ComputerError::InvalidArgs(
                "type_text: selector, when present, must not be empty".into(),
            ));
        }
        let mut params = json!({
            "text": text,
            "submit": submit,
            "clearFirst": clear_first,
        });
        if let Some(sel) = selector {
            params["selector"] = json!(sel.trim());
        }
        if let Some(t) = tab_id {
            self.validate_tab_id(&t, "type_text")?;
            params["tabId"] = t;
        }
        self.rpc("browser.type", params).await
    }

    /// `browser.scroll {deltaY?,deltaX?}` — omitted options fall back to
    /// the bridge defaults (`500` / `0`).
    pub async fn scroll(
        &self,
        delta_y: Option<f64>,
        delta_x: Option<f64>,
    ) -> Result<Value, ComputerError> {
        let mut params = Map::new();
        if let Some(dy) = delta_y.filter(|v| v.is_finite()) {
            params.insert("deltaY".into(), json!(dy));
        }
        if let Some(dx) = delta_x.filter(|v| v.is_finite()) {
            params.insert("deltaX".into(), json!(dx));
        }
        self.rpc("browser.scroll", Value::Object(params)).await
    }

    /// `browser.screenshot {format:"png",fullPage}` → raw data URL + decoded
    /// bytes. Non-decodable payloads fail with an Rpc-internal error rather
    /// than fabricating image data.
    pub async fn screenshot_data_url(
        &self,
        full_page: bool,
    ) -> Result<ScreenshotDataUrl, ComputerError> {
        let res = self
            .rpc(
                "browser.screenshot",
                json!({ "format": "png", "fullPage": full_page }),
            )
            .await?;
        let data_url =
            opt_str(&res, &["dataUrl", "data_url"]).ok_or_else(|| ComputerError::Rpc {
                code: -32603,
                message: "screenshot: engine result has no dataUrl member".to_string(),
            })?;
        let bytes = decode_data_url(&data_url)?;
        Ok(ScreenshotDataUrl { data_url, bytes })
    }

    /// Decoded PNG bytes ([`Self::screenshot_data_url`] minus the wrapper).
    pub async fn screenshot_png(&self, full_page: bool) -> Result<Vec<u8>, ComputerError> {
        Ok(self.screenshot_data_url(full_page).await?.bytes)
    }

    /// `browser.evaluate {expression}` → the inner `result` value.
    pub async fn evaluate_js(&self, expression: &str) -> Result<Value, ComputerError> {
        if blank(expression) {
            return Err(ComputerError::InvalidArgs(
                "evaluate_js: expression must not be empty".into(),
            ));
        }
        let res = self
            .rpc("browser.evaluate", json!({ "expression": expression }))
            .await?;
        Ok(res.get("result").cloned().unwrap_or(Value::Null))
    }

    /// `browser.getMarkdown {}` → page Markdown (empty string when absent).
    pub async fn markdown(&self) -> Result<String, ComputerError> {
        let res = self.rpc("browser.getMarkdown", json!({})).await?;
        Ok(opt_str(&res, &["markdown"]).unwrap_or_default())
    }

    /// `browser.getScrapedData {}` → the `data` object
    /// (`meta`, `tables`, `links{internal,external,total}`, `forms`, `outline`).
    pub async fn scraped_data(&self) -> Result<Value, ComputerError> {
        let res = self.rpc("browser.getScrapedData", json!({})).await?;
        Ok(res.get("data").cloned().unwrap_or(res))
    }

    /// `browser.getAccessibilityTree {}` → the full `{tree,formatted}`
    /// payload (deeply nested; left as raw JSON deliberately).
    pub async fn accessibility_tree(&self) -> Result<Value, ComputerError> {
        self.rpc("browser.getAccessibilityTree", json!({})).await
    }

    /// `browser.cdpCall {method,params?}` → the inner CDP `result`.
    pub async fn cdp_call(
        &self,
        method: &str,
        params: Option<Value>,
    ) -> Result<Value, ComputerError> {
        if blank(method) {
            return Err(ComputerError::InvalidArgs(
                "cdp_call: method must not be empty".into(),
            ));
        }
        let mut body = json!({ "method": method });
        if let Some(p) = params {
            body["params"] = p;
        }
        let res = self.rpc("browser.cdpCall", body).await?;
        Ok(res.get("result").cloned().unwrap_or(Value::Null))
    }

    /// `browser.getNetworkLogs {tabId?}` → log entries (empty when none).
    pub async fn network_logs(&self, tab_id: Option<Value>) -> Result<Vec<Value>, ComputerError> {
        if let Some(t) = &tab_id {
            self.validate_tab_id(t, "network_logs")?;
        }
        let res = self.rpc("browser.getNetworkLogs", wrap_tab(tab_id)).await?;
        Ok(res
            .get("logs")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default())
    }

    /// `browser.getConsoleLogs {tabId?}` → log entries (empty when none).
    pub async fn console_logs(&self, tab_id: Option<Value>) -> Result<Vec<Value>, ComputerError> {
        if let Some(t) = &tab_id {
            self.validate_tab_id(t, "console_logs")?;
        }
        let res = self.rpc("browser.getConsoleLogs", wrap_tab(tab_id)).await?;
        Ok(res
            .get("logs")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default())
    }

    /// `agent.runTask {goal,settings?}` → immediate start ack only.
    ///
    /// Progress events ride the bridge's WebSocket channel and are out of
    /// scope for v1; observe effects through [`Self::tabs`], logs or
    /// screenshots instead.
    pub async fn run_task(
        &self,
        goal: &str,
        settings: Option<Value>,
    ) -> Result<RunTaskStarted, ComputerError> {
        if blank(goal) {
            return Err(ComputerError::InvalidArgs(
                "run_task: goal must not be empty".into(),
            ));
        }
        let mut params = json!({ "goal": goal });
        if let Some(s) = settings {
            params["settings"] = s;
        }
        Ok(RunTaskStarted::parse(
            self.rpc("agent.runTask", params).await?,
        ))
    }

    fn validate_tab_id(&self, tab_id: &Value, caller: &str) -> Result<(), ComputerError> {
        if tab_id.is_null() {
            return Err(ComputerError::InvalidArgs(format!(
                "{caller}: tabId must not be null"
            )));
        }
        Ok(())
    }
}

fn wrap_tab(tab_id: Option<Value>) -> Value {
    match tab_id {
        Some(t) => json!({ "tabId": t }),
        None => json!({}),
    }
}

// ---------------------------------------------------------------------------
// BrowserTool — drop-in real replacement for ai-tools' simulated browser
// ---------------------------------------------------------------------------

/// Single action-dispatch tool driving the real browser through
/// [`OmniChromeClient`]. Actions: `status`, `tabs`, `navigate`, `click`,
/// `type`, `scroll`, `screenshot`, `markdown`, `scrape`, `evaluate`,
/// `a11y_tree`, `cdp`, `network_logs`, `console_logs`.
///
/// Output is compact JSON with data URLs truncated to ≤80 characters;
/// `screenshot` writes full PNG bytes when `savePath` is provided.
#[derive(Debug, Clone)]
pub struct BrowserTool {
    client: Arc<OmniChromeClient>,
}

impl BrowserTool {
    pub fn new(client: Arc<OmniChromeClient>) -> Self {
        Self { client }
    }
}

/// Client-side argument failure surfaced as a typed tool error (never sent
/// on the wire).
fn invalid_arg(message: impl Into<String>) -> AiError {
    AiError::Tool(ToolError::new(TOOL_NAME, message.into()))
}

fn arg_str<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key).and_then(Value::as_str)
}

fn arg_bool(args: &Value, key: &str) -> Option<bool> {
    args.get(key).and_then(Value::as_bool)
}

fn arg_f64(args: &Value, key: &str) -> Option<f64> {
    args.get(key).and_then(Value::as_f64)
}

fn arg_tab_id(args: &Value) -> Option<Value> {
    match args.get("tabId") {
        Some(v) if !v.is_null() => Some(v.clone()),
        _ => None,
    }
}

#[async_trait]
impl Tool for BrowserTool {
    fn name(&self) -> &str {
        TOOL_NAME
    }

    fn description(&self) -> &str {
        "Drive the real user's Chrome via the OmniChrome bridge: tab management, navigation, \
         clicks, typing, scrolling, PNG screenshots, Markdown/DOM scraping, page-JS evaluation, \
         the accessibility tree, raw CDP calls, and network/console logs. Fails with an honest \
         error when the bridge or extension is not running."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": [
                        "status", "tabs", "navigate", "click", "type", "scroll",
                        "screenshot", "markdown", "scrape", "evaluate", "a11y_tree",
                        "cdp", "network_logs", "console_logs"
                    ],
                    "description": "The browser action to execute"
                },
                "url": { "type": "string", "description": "Target URL (navigate)" },
                "selector": { "type": "string", "description": "CSS selector (click/type)" },
                "text": { "type": "string", "description": "Text to type (type)" },
                "x": { "type": "number", "description": "Viewport X coordinate (click)" },
                "y": { "type": "number", "description": "Viewport Y coordinate (click)" },
                "tabId": {
                    "description": "Opaque tab id from status/tabs (number or string)"
                },
                "submit": { "type": "boolean", "default": false, "description": "Press Enter after typing (type)" },
                "clearFirst": { "type": "boolean", "default": false, "description": "Clear the field before typing (type)" },
                "deltaX": { "type": "number", "description": "Horizontal scroll amount (scroll; bridge default 0)" },
                "deltaY": { "type": "number", "description": "Vertical scroll amount (scroll; bridge default 500)" },
                "fullPage": { "type": "boolean", "default": false, "description": "Capture beyond the viewport (screenshot)" },
                "savePath": {
                    "type": "string",
                    "description": "Filesystem path to write the full PNG bytes (screenshot)"
                },
                "expression": { "type": "string", "description": "JavaScript expression (evaluate)" },
                "cdpMethod": { "type": "string", "description": "Raw CDP method name (cdp)" },
                "cdpParams": { "type": "object", "description": "Raw CDP parameters object (cdp)" }
            },
            "required": ["action"]
        })
    }

    fn required_permissions(&self) -> Vec<&'static str> {
        // Static by contract: fs:write is required up-front even though only
        // `screenshot`+`savePath` touches the disk (rationale in module docs).
        vec!["net:http", "fs:write"]
    }

    async fn execute(
        &self,
        arguments: Value,
        _context: &ToolContext,
    ) -> Result<ToolOutput, AiError> {
        let action = arg_str(&arguments, "action")
            .ok_or_else(|| invalid_arg("missing required string argument `action`"))?;

        match action {
            "status" => {
                let info = self.client.status().await.map_err(AiError::from)?;
                let payload = json!({
                    "action": "status",
                    "status": info.status,
                    "connectedTabs": info.connected_tabs,
                    "activeTab": info.active_tab.map(|t| json!({
                        "id": t.id, "url": t.url, "title": t.title
                    })),
                });
                Ok(ToolOutput::ok(compact_json(&payload)))
            }
            "tabs" => {
                let tabs = self.client.tabs().await.map_err(AiError::from)?;
                let list: Vec<Value> = tabs
                    .iter()
                    .map(|t| {
                        json!({"id": t.id, "url": t.url, "title": t.title,
                               "active": t.active, "muted": t.muted})
                    })
                    .collect();
                Ok(ToolOutput::ok(compact_json(
                    &json!({ "action": "tabs", "tabs": list }),
                )))
            }
            "navigate" => {
                let url = arg_str(&arguments, "url").unwrap_or("");
                let res = self.client.navigate(url).await.map_err(AiError::from)?;
                let payload = json!({
                    "action": "navigate",
                    "success": res.success,
                    "url": res.url,
                    "tabId": res.tab_id,
                });
                Ok(ToolOutput::ok(compact_json(&payload)))
            }
            "click" => {
                // Validate BEFORE any network call: neither target ⇒ InvalidArgs.
                let tab_id = arg_tab_id(&arguments);
                let res = if let (Some(x), Some(y)) =
                    (arg_f64(&arguments, "x"), arg_f64(&arguments, "y"))
                {
                    self.client
                        .click_xy(x, y, tab_id)
                        .await
                        .map_err(AiError::from)?
                } else if let Some(selector) = arg_str(&arguments, "selector") {
                    self.client
                        .click_selector(selector, tab_id)
                        .await
                        .map_err(AiError::from)?
                } else {
                    return Err(invalid_arg(
                        "click requires either numeric `x`+`y` or a `selector`",
                    ));
                };
                let clicked_at = res.clicked_at.map(|(cx, cy)| json!({"x": cx, "y": cy}));
                let payload = json!({
                    "action": "click",
                    "success": res.success,
                    "x": res.x,
                    "y": res.y,
                    "clickedAt": clicked_at,
                });
                Ok(ToolOutput::ok(compact_json(&payload)))
            }
            "type" => {
                let text = arg_str(&arguments, "text").unwrap_or("");
                let submit = arg_bool(&arguments, "submit").unwrap_or(false);
                let clear_first = arg_bool(&arguments, "clearFirst").unwrap_or(false);
                let res = self
                    .client
                    .type_text(
                        text,
                        arg_str(&arguments, "selector"),
                        submit,
                        clear_first,
                        arg_tab_id(&arguments),
                    )
                    .await
                    .map_err(AiError::from)?;
                Ok(ToolOutput::ok(compact_json(
                    &json!({ "action": "type", "result": res }),
                )))
            }
            "scroll" => {
                let res = self
                    .client
                    .scroll(arg_f64(&arguments, "deltaY"), arg_f64(&arguments, "deltaX"))
                    .await
                    .map_err(AiError::from)?;
                Ok(ToolOutput::ok(compact_json(
                    &json!({ "action": "scroll", "result": res }),
                )))
            }
            "screenshot" => {
                let full_page = arg_bool(&arguments, "fullPage").unwrap_or(false);
                let shot = self
                    .client
                    .screenshot_data_url(full_page)
                    .await
                    .map_err(AiError::from)?;
                let mut payload = json!({
                    "action": "screenshot",
                    "format": "png",
                    "fullPage": full_page,
                    "bytes": shot.bytes.len(),
                    // Truncated to ≤80 chars; full bytes only ever hit savePath.
                    "dataUrlPreview": truncate_chars(&shot.data_url, DATA_URL_PREVIEW_CHARS),
                });
                if let Some(path) = arg_str(&arguments, "savePath") {
                    tokio::fs::write(path, &shot.bytes).await.map_err(|e| {
                        invalid_arg(format!("failed to write screenshot to `{path}`: {e}"))
                    })?;
                    payload["savedTo"] = json!(path);
                }
                Ok(ToolOutput::ok(payload.to_string()))
            }
            "markdown" => {
                let md = self.client.markdown().await.map_err(AiError::from)?;
                Ok(ToolOutput::ok(compact_json(
                    &json!({ "action": "markdown", "markdown": md }),
                )))
            }
            "scrape" => {
                let data = self.client.scraped_data().await.map_err(AiError::from)?;
                Ok(ToolOutput::ok(compact_json(
                    &json!({ "action": "scrape", "data": data }),
                )))
            }
            "evaluate" => {
                let expression = arg_str(&arguments, "expression").unwrap_or("");
                let result = self
                    .client
                    .evaluate_js(expression)
                    .await
                    .map_err(AiError::from)?;
                Ok(ToolOutput::ok(compact_json(
                    &json!({ "action": "evaluate", "result": result }),
                )))
            }
            "a11y_tree" => {
                let tree = self
                    .client
                    .accessibility_tree()
                    .await
                    .map_err(AiError::from)?;
                Ok(ToolOutput::ok(compact_json(
                    &json!({ "action": "a11y_tree", "tree": tree }),
                )))
            }
            "cdp" => {
                let method = arg_str(&arguments, "cdpMethod").unwrap_or("");
                let params = arguments
                    .get("cdpParams")
                    .cloned()
                    .filter(|p| p.is_object());
                if arguments
                    .get("cdpParams")
                    .map(|p| !p.is_object())
                    .unwrap_or(false)
                {
                    return Err(invalid_arg("cdpParams must be a JSON object when present"));
                }
                let result = self
                    .client
                    .cdp_call(method, params)
                    .await
                    .map_err(AiError::from)?;
                Ok(ToolOutput::ok(compact_json(
                    &json!({ "action": "cdp", "result": result }),
                )))
            }
            "network_logs" | "console_logs" => {
                let tab_id = arg_tab_id(&arguments);
                let logs = if action == "network_logs" {
                    self.client
                        .network_logs(tab_id)
                        .await
                        .map_err(AiError::from)?
                } else {
                    self.client
                        .console_logs(tab_id)
                        .await
                        .map_err(AiError::from)?
                };
                Ok(ToolOutput::ok(compact_json(
                    &json!({ "action": action, "logs": logs }),
                )))
            }
            other => Err(invalid_arg(format!(
                "unknown action `{other}`; see input_schema for the supported enum"
            ))),
        }
    }
}

// ---------------------------------------------------------------------------
// Offline unit tests (pure logic — no sockets)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn lookup<'a>(map: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        let map: HashMap<&str, &str> = map.iter().copied().collect();
        move |k: &str| map.get(k).map(|v| (*v).to_string()).filter(|v| !blank(v))
    }

    #[test]
    fn endpoint_resolution_prefers_env() {
        assert_eq!(endpoint_from(lookup(&[])), DEFAULT_BRIDGE_URL);
        assert_eq!(
            endpoint_from(lookup(&[(ENV_URL, "http://127.0.0.1:9999/rpc")])),
            "http://127.0.0.1:9999/rpc"
        );
        // Blank env values count as unset.
        assert_eq!(
            endpoint_from(lookup(&[(ENV_URL, "   ")])),
            DEFAULT_BRIDGE_URL
        );
    }

    #[test]
    fn token_file_candidates_follow_precedence() {
        assert_eq!(token_file_path(lookup(&[])), None);
        let explicit = PathBuf::from("/tmp/t.token");
        assert_eq!(
            token_file_path(lookup(&[(ENV_TOKEN_FILE, "/tmp/t.token")])),
            Some(explicit.clone())
        );
        // TOKEN_FILE beats HOME-derived root.
        assert_eq!(
            token_file_path(lookup(&[
                (ENV_TOKEN_FILE, "/tmp/t.token"),
                (ENV_HOME, "/ohm")
            ])),
            Some(explicit)
        );
        assert_eq!(
            token_file_path(lookup(&[(ENV_HOME, "/ohm")])),
            Some(PathBuf::from("/ohm/server/.bridge-token"))
        );
    }

    #[test]
    fn health_url_strips_rpc_suffix() {
        assert_eq!(
            health_url("http://localhost:8765/rpc"),
            "http://localhost:8765/health"
        );
        assert_eq!(
            health_url("http://localhost:8765"),
            "http://localhost:8765/health"
        );
        assert_eq!(
            health_url(DEFAULT_BRIDGE_URL),
            "http://localhost:8765/health"
        );
    }

    #[test]
    fn base64_known_vectors_decode() {
        assert_eq!(base64_decode("").unwrap(), Vec::<u8>::new());
        assert_eq!(base64_decode("SGVsbG8=").unwrap(), b"Hello".to_vec());
        assert_eq!(base64_decode("SGVsbG8h").unwrap(), b"Hello!".to_vec());
        assert_eq!(
            base64_decode("SGVs\nbG8g\r\nV29ybGQ=").unwrap(),
            b"Hello World".to_vec()
        );
        assert!(base64_decode("a!bc").is_none());
    }

    #[test]
    fn tiny_png_constant_decodes_with_magic() {
        const TINY_PNG_B64: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==";
        let bytes = base64_decode(TINY_PNG_B64).expect("constant must decode");
        assert!(bytes.starts_with(b"\x89PNG\r\n\x1a\n"));
        let from_wire = decode_data_url(&format!("data:image/png;base64,{TINY_PNG_B64}")).unwrap();
        assert_eq!(from_wire, bytes);
        // Bare base64 without the data: prefix also decodes (lenient path).
        assert_eq!(decode_data_url(TINY_PNG_B64).unwrap(), bytes);
        assert!(decode_data_url("data:image/png;base64,@@@").is_err());
    }

    #[test]
    fn sanitizer_bounds_data_urls_and_long_strings() {
        let long = "x".repeat(5000);
        let v = json!({
            "dataUrl": format!("data:image/png;base64,{}", "A".repeat(2000)),
            "nested": { "list": [long.as_str()] },
            "short": "keep me",
        });
        let out = sanitize_for_content(&v);
        let preview = out["dataUrl"].as_str().unwrap();
        assert!(preview.starts_with("data:image/png;base64,AAAA"));
        assert!(preview.chars().count() <= 90, "{preview}");
        assert!(out["nested"]["list"][0].as_str().unwrap().contains('…'));
        assert_eq!(out["short"].as_str().unwrap(), "keep me");
    }

    #[test]
    fn truncate_chars_keeps_short_strings_intact() {
        assert_eq!(truncate_chars("hello", 10), "hello");
        let t = truncate_chars(&"y".repeat(100), 72);
        assert!(t.starts_with(&"y".repeat(72)));
        assert!(t.ends_with("…+100"));
    }

    #[test]
    fn schema_enumerates_all_documented_actions() {
        let tool = BrowserTool::new(Arc::new(OmniChromeClient::new(DEFAULT_BRIDGE_URL, None)));
        let schema = tool.input_schema();
        let enum_values: Vec<&str> = schema["properties"]["action"]["enum"]
            .as_array()
            .unwrap()
            .iter()
            .map(Value::as_str)
            .map(Option::unwrap)
            .collect();
        assert_eq!(
            enum_values,
            [
                "status",
                "tabs",
                "navigate",
                "click",
                "type",
                "scroll",
                "screenshot",
                "markdown",
                "scrape",
                "evaluate",
                "a11y_tree",
                "cdp",
                "network_logs",
                "console_logs"
            ]
        );
        let mut perms = tool.required_permissions();
        perms.sort_unstable();
        assert_eq!(perms, vec!["fs:write", "net:http"]);
    }

    #[tokio::test]
    async fn client_side_validation_never_touches_network() {
        // Port 1 refuses instantly; any attempt to dial would error with
        // EngineUnreachable, never InvalidArgs.
        let client = OmniChromeClient::new("http://127.0.0.1:1/rpc", None);
        for err in [
            client.click_xy(f64::NAN, 0.0, None).await.err().unwrap(),
            client.click_selector("  ", None).await.err().unwrap(),
            client
                .type_text("", None, false, false, None)
                .await
                .err()
                .unwrap(),
            client.evaluate_js("  ").await.err().unwrap(),
            client.cdp_call("", None).await.err().unwrap(),
            client.run_task("", None).await.err().unwrap(),
        ] {
            assert!(matches!(err, ComputerError::InvalidArgs(_)), "{err}");
        }
        let tool = BrowserTool::new(Arc::new(client));
        let ctx = ToolContext::default();
        let err = tool
            .execute(json!({"action":"click"}), &ctx)
            .await
            .unwrap_err();
        assert!(
            err.to_string()
                .contains("either numeric `x`+`y` or a `selector`"),
            "{err}"
        );
        let err = tool
            .execute(json!({"action":"type"}), &ctx)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("invalid arguments"), "{err}");
        let err = tool
            .execute(json!({"action":"wat"}), &ctx)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("unknown action"), "{err}");
    }
}
