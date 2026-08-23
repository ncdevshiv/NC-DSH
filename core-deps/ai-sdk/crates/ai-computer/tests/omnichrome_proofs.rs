//! Offline wire proofs for the OmniChrome browser plugin.
//!
//! Every proof runs against a minimal handcrafted HTTP/1.1 JSON-RPC mock
//! (raw `tokio::net::TcpListener`, no HTTP crate): requests are read until
//! `\r\n\r\n`, the `Content-Length` body is drained, a canned response is
//! written, and the socket closes (`Connection: close`). The mock records
//! every request — path, `Authorization` header, raw body — so proofs can
//! assert *exact wire behavior*: verbatim method casing, camelCase params,
//! bearer auth presence, and zero-dial guarantees for client-side
//! validation. No real browser, extension, or bridge is required.

use std::sync::Arc;
use std::time::Duration;

use ai_computer::jsonrpc_client::ComputerError;
use ai_computer::omnichrome::{BrowserTool, OmniChromeClient};
use ai_tools::{Tool, ToolContext};
use parking_lot::Mutex;
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;

/// Smallest well-formed 1×1 PNG (magic `\x89PNG\r\n\x1a\n` + IHDR…IEND).
const TINY_PNG_B64: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==";
const PNG_MAGIC: &[u8] = b"\x89PNG\r\n\x1a\n";

// ---------------------------------------------------------------------------
// Minimal HTTP/1.1 JSON-RPC mock
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct RecordedRequest {
    method_verb: String,
    path: String,
    authorization: Option<String>,
    body: String,
}

impl RecordedRequest {
    /// The JSON-RPC envelope sent by the client.
    fn envelope(&self) -> Value {
        serde_json::from_str(&self.body).expect("client sends valid JSON")
    }
}

struct MockResponse {
    status: u16,
    body: String,
}

fn rpc_ok(result: Value) -> MockResponse {
    MockResponse {
        status: 200,
        body: json!({ "jsonrpc": "2.0", "id": 1, "result": result }).to_string(),
    }
}

fn rpc_error(status: u16, code: i64, message: &str) -> MockResponse {
    MockResponse {
        status,
        body: json!({
            "jsonrpc": "2.0", "id": 1,
            "error": { "code": code, "message": message }
        })
        .to_string(),
    }
}

type Handler = Arc<dyn Fn(&RecordedRequest) -> MockResponse + Send + Sync>;

struct MockServer {
    endpoint: String,
    requests: Arc<Mutex<Vec<RecordedRequest>>>,
    accept_loop: JoinHandle<()>,
}

impl MockServer {
    /// Binds `127.0.0.1:0`, serves one canned response per connection.
    async fn spawn(handler: Handler) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind ephemeral port");
        let port = listener.local_addr().expect("local addr").port();
        let requests: Arc<Mutex<Vec<RecordedRequest>>> = Arc::default();
        let worker_requests = Arc::clone(&requests);
        let accept_loop = tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let handler = Arc::clone(&handler);
                let requests = Arc::clone(&worker_requests);
                tokio::spawn(async move {
                    serve_connection(stream, handler, requests).await;
                });
            }
        });
        Self {
            endpoint: format!("http://127.0.0.1:{port}/rpc"),
            requests,
            accept_loop,
        }
    }

    fn client_with_token(&self, token: &str) -> OmniChromeClient {
        OmniChromeClient::new(self.endpoint.clone(), Some(token.to_string()))
    }

    fn request_count(&self) -> usize {
        self.requests.lock().len()
    }

    fn request(&self, index: usize) -> RecordedRequest {
        self.requests.lock()[index].clone()
    }

    fn last_request(&self) -> RecordedRequest {
        self.requests
            .lock()
            .last()
            .cloned()
            .expect("at least one recorded request")
    }
}

impl Drop for MockServer {
    fn drop(&mut self) {
        self.accept_loop.abort();
    }
}

async fn serve_connection(
    mut stream: TcpStream,
    handler: Handler,
    requests: Arc<Mutex<Vec<RecordedRequest>>>,
) {
    let Some(recorded) = read_request(&mut stream).await else {
        return;
    };
    requests.lock().push(recorded.clone());
    let response = handler(&recorded);
    let reason = match response.status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        500 => "Internal Server Error",
        _ => "OK",
    };
    let wire = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {len}\r\nConnection: close\r\n\r\n{body}",
        status = response.status,
        len = response.body.len(),
        body = response.body,
    );
    let _ = stream.write_all(wire.as_bytes()).await;
    let _ = stream.flush().await;
    let _ = stream.shutdown().await;
}

async fn read_request(stream: &mut TcpStream) -> Option<RecordedRequest> {
    let mut buf: Vec<u8> = Vec::new();
    let head_end = loop {
        if let Some(pos) = find_subslice(&buf, b"\r\n\r\n") {
            break pos + 4;
        }
        if !read_more(&mut buf, stream).await {
            return None;
        }
    };
    let head = String::from_utf8_lossy(&buf[..head_end]).to_string();
    let mut lines = head.lines();
    let request_line = lines.next().unwrap_or_default();
    let mut parts = request_line.split_whitespace();
    let verb = parts.next().unwrap_or_default().to_string();
    let path = parts.next().unwrap_or_default().to_string();

    let mut content_length = 0usize;
    let mut authorization = None;
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let name = name.trim();
        let value = value.trim();
        if name.eq_ignore_ascii_case("content-length") {
            content_length = value.parse().unwrap_or(0);
        } else if name.eq_ignore_ascii_case("authorization") {
            authorization = Some(value.to_string());
        }
    }

    let total = head_end + content_length;
    while buf.len() < total {
        if !read_more(&mut buf, stream).await {
            return None;
        }
    }
    Some(RecordedRequest {
        method_verb: verb,
        path,
        authorization,
        body: String::from_utf8_lossy(&buf[head_end..total]).to_string(),
    })
}

/// Reads once into `buf`; false on EOF/error/timeout (5 s guard so a broken
/// exchange can never hang the suite).
async fn read_more(buf: &mut Vec<u8>, stream: &mut TcpStream) -> bool {
    let mut chunk = [0u8; 8192];
    match tokio::time::timeout(Duration::from_secs(5), stream.read(&mut chunk)).await {
        Ok(Ok(n)) if n > 0 => {
            buf.extend_from_slice(&chunk[..n]);
            true
        }
        _ => false,
    }
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

// ---------------------------------------------------------------------------
// Proofs
// ---------------------------------------------------------------------------

/// (a) Happy-path navigate: bearer Authorization header present, JSON-RPC
/// envelope uses verbatim `browser.navigate` method + camelCase params,
/// and the typed result parses `{success,url,tabId}`.
#[tokio::test]
async fn happy_navigate_sends_bearer_and_exact_casing() {
    let server = MockServer::spawn(Arc::new(|req| {
        assert_eq!(req.method_verb, "POST");
        rpc_ok(json!({ "success": true, "url": "https://example.com/page", "tabId": 42 }))
    }))
    .await;

    let client = server.client_with_token("sekrit-token");
    let nav = client
        .navigate("https://example.com/page")
        .await
        .expect("navigate succeeds");
    assert_eq!(nav.success, Some(true));
    assert_eq!(nav.url.as_deref(), Some("https://example.com/page"));
    assert_eq!(nav.tab_id, Some(json!(42)));

    let recorded = server.last_request();
    assert_eq!(recorded.path, "/rpc");
    assert_eq!(
        recorded.authorization.as_deref(),
        Some("Bearer sekrit-token")
    );
    let envelope = recorded.envelope();
    assert_eq!(envelope["jsonrpc"], "2.0");
    assert!(envelope["id"].is_u64(), "numeric request id");
    assert_eq!(
        envelope["method"], "browser.navigate",
        "verbatim wire casing"
    );
    assert_eq!(envelope["params"]["url"], "https://example.com/page");
    assert_eq!(server.request_count(), 1);
}

/// (b) Screenshot: the dataUrl prefix is stripped and the base64 payload
/// decoded to real PNG bytes (magic-number check).
#[tokio::test]
async fn screenshot_dataurl_stripped_and_decoded_to_png() {
    let data_url = format!("data:image/png;base64,{TINY_PNG_B64}");
    let server =
        MockServer::spawn(Arc::new(move |_req| rpc_ok(json!({ "dataUrl": data_url })))).await;

    let client = server.client_with_token("t");
    let bytes = client.screenshot_png(false).await.expect("decodable PNG");
    assert!(
        bytes.starts_with(PNG_MAGIC),
        "decoded bytes are a PNG: {:02X?}",
        &bytes[..8.min(bytes.len())]
    );

    let envelope = server.last_request().envelope();
    assert_eq!(envelope["method"], "browser.screenshot");
    assert_eq!(envelope["params"]["format"], "png");
    assert_eq!(envelope["params"]["fullPage"], false);
}

/// (c) Click with neither coordinates nor selector fails with InvalidArgs
/// BEFORE any network traffic — the mock records zero requests.
#[tokio::test]
async fn click_without_target_fails_before_network() {
    let server = MockServer::spawn(Arc::new(|_| rpc_ok(json!({ "success": true })))).await;

    let client = server.client_with_token("t");
    let err = client.click_selector("", None).await.unwrap_err();
    assert!(matches!(err, ComputerError::InvalidArgs(_)), "{err}");
    let err = client.click_xy(1.0, f64::NAN, None).await.unwrap_err();
    assert!(matches!(err, ComputerError::InvalidArgs(_)), "{err}");

    let tool = BrowserTool::new(Arc::new(OmniChromeClient::new(
        server.endpoint.clone(),
        None,
    )));
    let ctx = ToolContext::default();
    // Dispatch-level rejection when NO target kind is present at all.
    for args in [
        json!({ "action": "click" }),
        json!({ "action": "click", "x": 10 }),
    ] {
        // y missing ⇒ no complete coordinate target
        let err = tool.execute(args, &ctx).await.unwrap_err();
        assert!(
            err.to_string()
                .contains("either numeric `x`+`y` or a `selector`"),
            "{err}"
        );
    }
    // Client-level rejection for a present-but-blank target.
    let err = tool
        .execute(json!({ "action": "click", "selector": "   " }), &ctx)
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("selector must not be empty"),
        "{err}"
    );

    assert_eq!(
        server.request_count(),
        0,
        "validation must prevent any dial-out"
    );
}

/// (d) HTTP 401 wrapping -32001 ⇒ `ComputerError::Unauthorized`.
#[tokio::test]
async fn http_401_maps_to_unauthorized() {
    let server =
        MockServer::spawn(Arc::new(|_| rpc_error(401, -32001, "invalid bridge token"))).await;
    let client = server.client_with_token("wrong-token");
    let err = client.status().await.unwrap_err();
    assert!(matches!(err, ComputerError::Unauthorized(_)), "{err}");
    assert!(err.to_string().contains("unauthorized"), "{err}");
    assert_eq!(server.request_count(), 1);
}

/// (e) HTTP 500 wrapping -32000 (bridge forwarding timeout) surfaces as
/// EngineUnreachable carrying an actionable "extension" hint.
#[tokio::test]
async fn http_500_minus_32000_surfaces_extension_hint() {
    let server = MockServer::spawn(Arc::new(|_| {
        rpc_error(
            500,
            -32000,
            "Timed out waiting for the extension to respond",
        )
    }))
    .await;
    let client = server.client_with_token("t");
    let err = client.navigate("https://example.com").await.unwrap_err();
    assert!(matches!(err, ComputerError::EngineUnreachable(_)), "{err}");
    let message = err.to_string();
    assert!(
        message.contains("extension"),
        "actionable hint expected: {message}"
    );
    assert!(
        message.contains("Timed out waiting"),
        "original cause retained: {message}"
    );
}

/// (f) Unknown method (-32601, HTTP 200) ⇒ `ComputerError::Rpc`.
#[tokio::test]
async fn unknown_method_maps_to_rpc_error() {
    let server = MockServer::spawn(Arc::new(|_| rpc_error(200, -32601, "Method not found"))).await;
    let client = server.client_with_token("t");
    let err = client.switch_tab(json!("tab-abc")).await.unwrap_err();
    match err {
        ComputerError::Rpc { code, message } => {
            assert_eq!(code, -32601);
            assert_eq!(message, "Method not found");
        }
        other => panic!("expected Rpc variant, got: {other:?}"),
    }
}

/// (g) Markdown roundtrip: engine payload survives client + tool layers
/// intact (modulo the oversized-string guard, which does not trigger here).
#[tokio::test]
async fn markdown_roundtrips_through_client_and_tool() {
    let markdown = "# Hello **world**\n\n- item [link](https://example.com)";
    let server =
        MockServer::spawn(Arc::new(move |_| rpc_ok(json!({ "markdown": markdown })))).await;

    let client = server.client_with_token("t");
    assert_eq!(client.markdown().await.expect("markdown"), markdown);

    let tool = BrowserTool::new(Arc::new(OmniChromeClient::new(
        server.endpoint.clone(),
        None,
    )));
    let output = tool
        .execute(json!({ "action": "markdown" }), &ToolContext::default())
        .await
        .expect("tool ok");
    assert!(!output.is_error);
    let value: Value = serde_json::from_str(&output.content).expect("compact JSON content");
    assert_eq!(value["markdown"], markdown);

    let envelope = server.last_request().envelope();
    assert_eq!(envelope["method"], "browser.getMarkdown");
}

/// Extra: `GET /health` bypasses JSON-RPC, carries the bearer header, and
/// parses into `HealthInfo`.
#[tokio::test]
async fn health_endpoint_carries_bearer_and_parses() {
    let server = MockServer::spawn(Arc::new(|_req| MockResponse {
        status: 200,
        body: json!({ "status": "ok", "uptimeMs": 12 }).to_string(),
    }))
    .await;
    let client = server.client_with_token("health-token");
    let health = client.health().await.expect("healthy");
    assert_eq!(health.status.as_deref(), Some("ok"));

    let recorded = server.last_request();
    assert_eq!(recorded.path, "/health", "derived from the /rpc endpoint");
    assert_eq!(
        recorded.authorization.as_deref(),
        Some("Bearer health-token")
    );
    assert_eq!(recorded.method_verb, "GET");
}

/// Extra: end-to-end tool dispatch over the mock — exact method casing for
/// getTabs/click/type, camelCase params (`clearFirst`), selector-click
/// `clickedAt` parsing.
#[tokio::test]
async fn tool_dispatch_end_to_end_over_mock() {
    let server = MockServer::spawn(Arc::new(|req| {
        match req.envelope()["method"].as_str().unwrap_or_default() {
            "browser.getTabs" => rpc_ok(json!([
                { "id": 1, "url": "https://a.example", "title": "A", "active": true, "muted": false },
                { "id": 2, "url": "https://b.example", "title": "B", "active": false, "muted": true }
            ])),
            "browser.click" => {
                let params = &req.envelope()["params"];
                if params["selector"] == "#submit-btn" {
                    rpc_ok(json!({ "success": true, "clickedAt": { "x": 12.5, "y": 34.0 } }))
                } else {
                    rpc_ok(json!({ "success": true, "x": 1.0, "y": 2.0 }))
                }
            }
            "browser.type" => rpc_ok(json!({ "success": true, "typed": 5 })),
            other => panic!("unexpected method on the wire: {other}"),
        }
    }))
    .await;

    let tool = BrowserTool::new(Arc::new(OmniChromeClient::new(
        server.endpoint.clone(),
        None,
    )));
    let ctx = ToolContext::default();

    let out = tool
        .execute(json!({ "action": "tabs" }), &ctx)
        .await
        .expect("tabs");
    let value: Value = serde_json::from_str(&out.content).unwrap();
    assert_eq!(value["tabs"].as_array().map(Vec::len), Some(2));
    assert_eq!(value["tabs"][1]["muted"], true);

    let out = tool
        .execute(
            json!({ "action": "click", "selector": "#submit-btn" }),
            &ctx,
        )
        .await
        .expect("click");
    let value: Value = serde_json::from_str(&out.content).unwrap();
    assert_eq!(value["clickedAt"]["x"], 12.5);

    let out = tool
        .execute(
            json!({ "action": "type", "selector": "input#q", "text": "hello", "clearFirst": true, "submit": true }),
            &ctx,
        )
        .await
        .expect("type");
    let value: Value = serde_json::from_str(&out.content).unwrap();
    assert_eq!(value["result"]["typed"], 5);

    let methods: Vec<String> = {
        let lock = server.requests.lock();
        lock.iter()
            .map(|r| {
                r.envelope()["method"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string()
            })
            .collect()
    };
    assert_eq!(
        methods,
        ["browser.getTabs", "browser.click", "browser.type"]
    );
    let type_params = server.request(2).envelope()["params"].clone();
    assert_eq!(type_params["text"], "hello");
    assert_eq!(type_params["clearFirst"], true, "camelCase passthrough");
    assert_eq!(type_params["submit"], true);
    assert_eq!(type_params["selector"], "input#q");
}

/// Extra: `agent.runTask` yields the immediate started ack; progress is
/// WebSocket-only and deliberately out of scope in v1 (documented contract).
#[tokio::test]
async fn run_task_returns_started_ack_only() {
    let server = MockServer::spawn(Arc::new(|_| {
        rpc_ok(json!({ "status": "started", "goal": "collect headlines", "tabId": "t-9" }))
    }))
    .await;
    let client = server.client_with_token("t");
    let ack = client
        .run_task("collect headlines", Some(json!({ "maxSteps": 3 })))
        .await
        .expect("started");
    assert_eq!(ack.status.as_deref(), Some("started"));
    assert_eq!(ack.goal.as_deref(), Some("collect headlines"));
    assert_eq!(ack.tab_id, Some(json!("t-9")));

    let envelope = server.last_request().envelope();
    assert_eq!(envelope["method"], "agent.runTask");
    assert_eq!(envelope["params"]["goal"], "collect headlines");
    assert_eq!(envelope["params"]["settings"]["maxSteps"], 3);
}

/// Extra: screenshot + savePath writes the FULL decoded bytes to disk while
/// tool content keeps only the ≤80-char preview. (Permissions are enforced
/// by `run_tool`; these proofs call `execute` directly.)
#[tokio::test]
async fn screenshot_save_path_writes_full_bytes_with_truncated_content() {
    let data_url = format!("data:image/png;base64,{TINY_PNG_B64}");
    let server = MockServer::spawn(Arc::new(move |_| rpc_ok(json!({ "dataUrl": data_url })))).await;

    let path = std::env::temp_dir().join(format!(
        "omnichrome_proof_{}_{}.png",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let tool = BrowserTool::new(Arc::new(OmniChromeClient::new(
        server.endpoint.clone(),
        None,
    )));
    let out = tool
        .execute(json!({ "action": "screenshot", "fullPage": true, "savePath": path.display().to_string() }), &ToolContext::default())
        .await
        .expect("screenshot ok");

    let value: Value = serde_json::from_str(&out.content).unwrap();
    let preview = value["dataUrlPreview"].as_str().expect("preview present");
    assert!(
        preview.chars().count() <= 80,
        "preview ≤80 chars: {preview}"
    );
    let saved_len = value["bytes"].as_u64().expect("byte count") as usize;
    assert_eq!(value["savedTo"], json!(path.display().to_string()));

    let written = std::fs::read(&path).expect("file written");
    assert_eq!(written.len(), saved_len);
    assert!(written.starts_with(PNG_MAGIC));
    std::fs::remove_file(&path).ok();
}
