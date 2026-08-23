//! Live end-to-end proof against a RUNNING Native Computer Use engine.
//!
//! Gated: skipped unless `COMPUTERUSE_SERVER_URL` or the default endpoint
//! answers `/health`. Run with:
//! ```bash
//! cargo test -p ai-computer --test computer_live -- --ignored --nocapture
//! ```

use ai_computer::native::{ComputerTool, NativeComputerClient};
use ai_tools::{Tool, ToolContext};
use serde_json::json;

async fn client() -> Option<NativeComputerClient> {
    // Probe health first; absent engine ⇒ skip (never fabricate).
    let c = NativeComputerClient::with_env();
    match c.health().await {
        Ok(h) if h["status"] == "online" => Some(c),
        _ => None,
    }
}

#[tokio::test]
#[ignore = "requires the NativeServer running on localhost:8888"]
async fn live_status_screenshot_and_ocr() {
    let Some(client) = client().await else {
        eprintln!("SKIP: NativeServer not reachable on :8888");
        return;
    };

    // 1. Status: cursor + active window telemetry.
    let status = client.status().await.expect("status");
    assert_eq!(status["status"], "online");
    eprintln!("status ok: {}", status);

    // 2. Screenshot bytes decode to a real PNG.
    match client.screenshot(None, true).await.expect("screenshot") {
        ai_computer::native::ScreenshotResult::Bytes(bytes) => {
            assert!(bytes.len() > 1000, "screenshot suspiciously small");
            assert!(bytes.starts_with(b"\x89PNG\r\n\x1a\n"), "PNG magic missing");
            eprintln!("screenshot ok: {} bytes PNG", bytes.len());
        }
        other => panic!("expected decoded bytes, got {other:?}"),
    }

    // 3. OCR extraction runs (array; may be empty on minimal desktops).
    let ocr = client.ocr_extract_all().await.expect("ocr");
    let n = ocr.as_array().map(|a| a.len()).unwrap_or(0);
    eprintln!("ocr ok: {n} text regions");

    // 4. Tool-layer dispatch round-trip.
    let tool = ComputerTool::new(client);
    let out = tool
        .execute(json!({ "action": "telemetry" }), &ToolContext::default())
        .await
        .expect("telemetry via tool");
    assert!(!out.is_error);
    assert!(out.content.contains("TotalMemoryMB"), "{}", out.content);
    eprintln!("PASS: live computer-use end-to-end ({})", out.content);
}
