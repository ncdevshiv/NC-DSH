//! Offline wire proofs for the Native Computer Use desktop plugin.
//!
//! Same handcrafted HTTP/1.1 mock approach as `omnichrome_proofs.rs`:
//! every proof asserts *exact wire behavior* — verbatim method names,
//! param casing, exact-string bearer auth, pre-network validation, and
//! the engine's quirks (error bodies with `id: null`, data-URL screenshots,
//! PascalCase OCR fields).

use std::sync::Arc;

use ai_computer::jsonrpc_client::ComputerError;
use ai_computer::native::{ComputerTool, NativeComputerClient};
use ai_tools::{Tool, ToolContext};
use parking_lot::Mutex;
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;

const TINY_PNG_DATA_URL: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==";

// ---------------------------------------------------------------------------
// Minimal HTTP/1.1 JSON-RPC mock (mirrors omnichrome_proofs)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct RecordedRequest {
    authorization: Option<String>,
    body: String,
}

impl RecordedRequest {
    fn envelope(&self) -> Value {
        serde_json::from_str(&self.body).expect("client sends valid JSON")
    }
}

struct MockResponse {
    status: u16,
    body: String,
}

fn rpc_ok(result: Value) -> MockResponse {
    // NativeServer echoes a non-standard top-level `agent` member on
    // success; the client must tolerate it.
    MockResponse {
        status: 200,
        body: json!({
            "jsonrpc": "2.0", "id": 1, "result": result,
            "agent": "agent-abcdef1234"
        })
        .to_string(),
    }
}

fn rpc_error_body(code: i64, message: &str) -> MockResponse {
    // Engine quirk: handler failures return id:null with HTTP 200.
    MockResponse {
        status: 200,
        body: json!({
            "jsonrpc": "2.0", "id": Value::Null,
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
    async fn spawn(handler: Handler) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let port = listener.local_addr().expect("addr").port();
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

    fn client(&self, token: Option<&str>) -> NativeComputerClient {
        NativeComputerClient::new(self.endpoint.clone(), token.map(|t| t.to_string()))
    }

    fn last_request(&self) -> RecordedRequest {
        self.requests.lock().last().cloned().expect("a request")
    }

    fn request_count(&self) -> usize {
        self.requests.lock().len()
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
    let head = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        response.status,
        if response.status == 200 { "OK" } else { "Err" },
        response.body.len()
    );
    let _ = stream.write_all(head.as_bytes()).await;
    let _ = stream.write_all(response.body.as_bytes()).await;
    let _ = stream.shutdown().await;
}

async fn read_request(stream: &mut TcpStream) -> Option<RecordedRequest> {
    let mut buf = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        match stream.read(&mut byte).await {
            Ok(1) => buf.push(byte[0]),
            _ => return None,
        }
        if buf.ends_with(b"\r\n\r\n") {
            break;
        }
    }
    let head = String::from_utf8_lossy(&buf).to_string();
    let content_length = head
        .lines()
        .find_map(|l| {
            let lower = l.to_ascii_lowercase();
            if lower.starts_with("content-length:") {
                l.split(':')
                    .nth(1)
                    .and_then(|v| v.trim().parse::<usize>().ok())
            } else {
                None
            }
        })
        .unwrap_or(0);
    let mut body = vec![0u8; content_length];
    if content_length > 0 {
        stream.read_exact(&mut body).await.ok()?;
    }
    let authorization = head.lines().find_map(|l| {
        let lower = l.to_ascii_lowercase();
        lower
            .starts_with("authorization:")
            .then(|| l.split_once(':').map(|(_, v)| v.trim().to_string()))
            .flatten()
    });
    Some(RecordedRequest {
        authorization,
        body: String::from_utf8_lossy(&body).to_string(),
    })
}

fn tool_on(server: &MockServer) -> ComputerTool {
    ComputerTool::new(server.client(Some("tok-123")))
}

// ---------------------------------------------------------------------------
// Proofs
// ---------------------------------------------------------------------------

#[tokio::test]
async fn click_sends_exact_method_params_and_bearer() {
    let server = MockServer::spawn(Arc::new(|_req| {
        rpc_ok(json!({ "success": true, "clickedAt": { "X": 500, "Y": 300 } }))
    }))
    .await;
    let tool = tool_on(&server);

    let out = tool
        .execute(
            json!({ "action": "click", "params": { "x": 500, "y": 300 } }),
            &ToolContext::default(),
        )
        .await
        .expect("click succeeds");

    assert!(!out.is_error);
    let req = server.last_request();
    assert_eq!(
        req.authorization.as_deref(),
        Some("Bearer tok-123"),
        "exact-string bearer auth"
    );
    let env = req.envelope();
    assert_eq!(env["method"], json!("computer.click"));
    assert_eq!(env["params"]["x"], json!(500));
    assert_eq!(env["params"]["y"], json!(300));
    assert!(
        out.content.contains("clickedAt"),
        "casing preserved: {}",
        out.content
    );
}

#[tokio::test]
async fn ocr_find_parses_pascal_case_result() {
    let server = MockServer::spawn(Arc::new(|req| {
        assert_eq!(req.envelope()["method"], json!("computer.ocr_find"));
        rpc_ok(json!({
            "Success": true, "Source": "UIAutomation", "Text": "Settings",
            "Bounds": { "X": 10, "Y": 20, "Width": 60, "Height": 22 },
            "Center": { "X": 40, "Y": 31 }
        }))
    }))
    .await;
    let tool = tool_on(&server);

    let out = tool
        .execute(
            json!({ "action": "ocr_find", "params": { "pattern": "Settings" } }),
            &ToolContext::default(),
        )
        .await
        .expect("ocr_find succeeds");

    assert!(out.content.contains("\"X\":40"), "{}", out.content);
}

#[tokio::test]
async fn screenshot_strips_data_url_and_decodes_png() {
    let server = MockServer::spawn(Arc::new(|_req| {
        rpc_ok(json!({ "success": true, "base64": TINY_PNG_DATA_URL }))
    }))
    .await;
    let tool = tool_on(&server);

    let out = tool
        .execute(json!({ "action": "screenshot" }), &ToolContext::default())
        .await
        .expect("screenshot succeeds");

    assert!(out.content.contains("pngMagic\":true"), "{}", out.content);
    assert!(
        !out.content.contains("iVBORw0KGgo"),
        "no raw base64 in output"
    );
}

#[tokio::test]
async fn type_without_target_is_invalid_before_any_dial() {
    let server = MockServer::spawn(Arc::new(|_req| {
        panic!("engine must never be dialed when target is missing")
    }))
    .await;
    let tool = tool_on(&server);

    let err = tool
        .execute(
            json!({ "action": "type", "params": { "text": "hi" } }),
            &ToolContext::default(),
        )
        .await
        .expect_err("missing target rejected");

    assert!(err.to_string().contains("target"), "{err}");
    assert_eq!(server.request_count(), 0, "zero network dials");
}

#[tokio::test]
async fn target_required_sentinel_surfaces_intact() {
    let server = MockServer::spawn(Arc::new(|_req| {
        rpc_error_body(-32000, "TARGET_REQUIRED: no target window resolved")
    }))
    .await;
    let tool = tool_on(&server);

    let err = tool
        .execute(
            json!({ "action": "paste", "params": { "text": "x", "target": "focused" } }),
            &ToolContext::default(),
        )
        .await
        .expect_err("engine rejection surfaces");

    assert!(err.to_string().contains("TARGET_REQUIRED"), "{err}");
}

#[tokio::test]
async fn http_401_maps_to_unauthorized() {
    let server = MockServer::spawn(Arc::new(|_req| MockResponse {
        status: 401,
        body: json!({"jsonrpc":"2.0","id":null,"error":{"code":-32001,"message":"Unauthorized. Provide 'Authorization: Bearer <token>' header."}}).to_string(),
    }))
    .await;
    let client = server.client(Some("wrong"));

    match client.status().await {
        Err(ComputerError::Unauthorized(m)) => {
            assert!(m.contains("401") || m.contains("Unauthorized"), "{m}")
        }
        other => panic!("expected Unauthorized, got {other:?}"),
    }
}

#[tokio::test]
async fn methods_discovery_parses_count() {
    let server = MockServer::spawn(Arc::new(|req| {
        // GET /methods carries no JSON-RPC envelope — assert auth instead.
        // Real engine responds with a PLAIN object (no result wrapper).
        assert_eq!(
            req.authorization.as_deref(),
            Some("Bearer tok"),
            "methods discovery must authenticate"
        );
        MockResponse {
            status: 200,
            body: json!({
                "methods": [ {"name":"computer.click"}, {"name":"computer.type"} ],
                "count": 2
            })
            .to_string(),
        }
    }))
    .await;
    let client = server.client(Some("tok"));

    let methods = client.methods().await.expect("methods succeeds");
    assert_eq!(methods["count"], json!(2));
}
