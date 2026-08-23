//! Realtime WebSocket transport: [`RealtimeConnection`] over tokio-tungstenite.
//!
//! Design notes:
//! - The underlying socket is split once at connect time into a sink half and
//!   a stream half, each behind an `Arc<tokio::sync::Mutex>` so `send`, `recv`
//!   and the derived event stream are task-safe and callable from `&self`.
//! - Text *and* binary WebSocket frames are mapped through
//!   [`RealtimeEventFramer`]: both carry JSON-encoded server events in this
//!   protocol (binary framing exists so providers can batch deltas efficiently).
//! - Unknown server event types never fail the stream — they decode into
//!   [`RealtimeServerEvent::Other`] (see `realtime.rs`).

use std::sync::Arc;

use futures::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::{HeaderName, HeaderValue};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

use ai_errors::{AiError, NetworkError};

use crate::realtime::{RealtimeClientEvent, RealtimeEventFramer, RealtimeServerEvent};

type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;
type WsSink = futures::stream::SplitSink<WsStream, Message>;
type WsSource = futures::stream::SplitStream<WsStream>;

/// A bidirectional, task-safe Realtime session connection.
///
/// Clone-free shared ownership: `send` may run concurrently with `recv` /
/// [`RealtimeConnection::events`] from other tasks.
#[derive(Clone)]
pub struct RealtimeConnection {
    url: String,
    sink: Arc<Mutex<WsSink>>,
    source: Arc<Mutex<WsSource>>,
}

impl RealtimeConnection {
    /// Opens a WebSocket connection to `url` (`ws://…`; TLS schemes require a
    /// tokio-tungstenite build with a TLS feature) and applies extra
    /// handshake `headers` (e.g. `Authorization`). Reserved `Sec-WebSocket-*`
    /// header names are ignored to keep the handshake valid.
    pub async fn connect(
        url: impl AsRef<str>,
        headers: &reqwest::header::HeaderMap,
    ) -> Result<Self, AiError> {
        let url = url.as_ref().to_string();
        let mut request = url
            .as_str()
            .into_client_request()
            .map_err(|e| AiError::Network(NetworkError::new("realtime connect", e.to_string())))?;
        for (name, value) in headers {
            let lower = name.as_str().to_ascii_lowercase();
            if lower.starts_with("sec-websocket")
                || matches!(lower.as_str(), "host" | "connection" | "upgrade")
            {
                continue;
            }
            let ok_name = HeaderName::from_bytes(lower.as_bytes());
            let ok_value = HeaderValue::from_bytes(value.as_bytes());
            match (ok_name, ok_value) {
                (Ok(n), Ok(v)) => {
                    request.headers_mut().insert(n, v);
                }
                _ => continue,
            }
        }
        let (ws_stream, _response) =
            tokio_tungstenite::connect_async(request)
                .await
                .map_err(|e| {
                    AiError::Network(NetworkError::new("realtime websocket", e.to_string()))
                })?;
        let (sink, source) = ws_stream.split();
        Ok(Self {
            url,
            sink: Arc::new(Mutex::new(sink)),
            source: Arc::new(Mutex::new(source)),
        })
    }

    /// The endpoint this connection was opened against.
    pub fn url(&self) -> &str {
        &self.url
    }

    /// Sends one client event as a JSON text frame.
    pub async fn send(&self, event: &RealtimeClientEvent) -> Result<(), AiError> {
        let json = RealtimeEventFramer::serialize_client_event(event)?;
        self.send_raw_text(json).await
    }

    /// Receives the next server event.
    ///
    /// `Ok(None)` means the peer closed the session cleanly. Text and binary
    /// frames are both parsed through the framer; protocol-level ping/pong is
    /// answered transparently by tungstenite.
    pub async fn recv(&self) -> Result<Option<RealtimeServerEvent>, AiError> {
        loop {
            let msg = {
                let mut source = self.source.lock().await;
                source.next().await
            };
            match msg {
                None => return Ok(None),
                Some(Err(e)) => {
                    return Err(AiError::Network(NetworkError::new(
                        "realtime receive",
                        e.to_string(),
                    )));
                }
                Some(Ok(Message::Close(_))) => return Ok(None),
                // Tungstenite auto-queues the pong; nothing surfaces here.
                Some(Ok(Message::Ping(_) | Message::Pong(_))) => continue,
                Some(Ok(Message::Text(txt))) => {
                    return RealtimeEventFramer::parse_server_event(txt.as_ref()).map(Some);
                }
                Some(Ok(Message::Binary(bytes))) => {
                    let payload = String::from_utf8(bytes).map_err(|e| {
                        AiError::Serialization(ai_errors::SerializationError::new(format!(
                            "realtime binary frame is not UTF-8 JSON: {e}"
                        )))
                    })?;
                    return RealtimeEventFramer::parse_server_event(&payload).map(Some);
                }
                Some(Ok(other)) => {
                    return Err(AiError::Network(NetworkError::new(
                        "realtime receive",
                        format!("unexpected frame type: {other:?}"),
                    )));
                }
            }
        }
    }

    /// Cancels the current server-side response (the barge-in send path).
    pub async fn cancel_response(&self, event_id: impl Into<String>) -> Result<(), AiError> {
        self.send(&RealtimeClientEvent::ResponseCancel {
            event_id: event_id.into(),
        })
        .await
    }

    /// Gracefully closes the session: emits a Close frame, then waits
    /// (bounded, 5 s) for the peer's close acknowledgement so the TCP socket
    /// ends in a clean shutdown rather than a reset.
    pub async fn close(&self) -> Result<(), AiError> {
        {
            let mut sink = self.sink.lock().await;
            sink.send(Message::Close(None)).await.map_err(|e| {
                AiError::Network(NetworkError::new("realtime close", e.to_string()))
            })?;
            sink.flush().await.map_err(|e| {
                AiError::Network(NetworkError::new("realtime close", e.to_string()))
            })?;
        }
        // Drain until the peer's close frame arrives (recv maps it to
        // Ok(None)), completing the RFC 6455 closing handshake.
        let _ = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            while let Ok(Some(_)) = self.recv().await {}
        })
        .await;
        Ok(())
    }

    /// Adapts this connection into a `Stream` of server events. Ends after a
    /// clean close (`Ok(None)` from [`recv`](Self::recv)); yields one final
    /// `Err` item before terminating if the transport fails.
    pub fn events(
        &self,
    ) -> futures::stream::BoxStream<'static, Result<RealtimeServerEvent, AiError>> {
        futures::stream::unfold((self.clone(), false), |(conn, failed)| async move {
            if failed {
                return None;
            }
            match conn.recv().await {
                Ok(Some(event)) => Some((Ok(event), (conn, false))),
                Ok(None) => None,
                Err(e) => Some((Err(e), (conn, true))),
            }
        })
        .boxed()
    }

    async fn send_raw_text(&self, json: String) -> Result<(), AiError> {
        let mut sink = self.sink.lock().await;
        sink.send(Message::Text(json))
            .await
            .map_err(|e| AiError::Network(NetworkError::new("realtime send", e.to_string())))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;
    use serde_json::Value;
    use std::collections::HashMap;
    use std::sync::Mutex as StdMutex;

    /// Spawns an in-test realtime server on a loopback port (first-party
    /// test infrastructure, mirroring `mcp_http.rs` conventions).
    ///
    /// Behavior per connection:
    /// - Every received client event is acknowledged with a
    ///   `response.text.delta` whose `response_id` echoes the client event's
    ///   `"type"` — proving full-duplex JSON round-trips.
    /// - An `input_audio_buffer.append` additionally triggers a **binary**
    ///   frame carrying a `response.audio.delta` JSON payload — proving
    ///   binary frames map through the framer identically to text frames.
    /// - A `response.cancel` triggers a `response.cancelled` interruption
    ///   signal followed by `response.done`.
    async fn run_echo_server<S>(mut ws: S)
    where
        S: futures::Stream<Item = Result<Message, tokio_tungstenite::tungstenite::Error>>
            + futures::Sink<Message, Error = tokio_tungstenite::tungstenite::Error>
            + Unpin,
    {
        while let Some(msg) = ws.next().await {
            match msg.expect("server socket error") {
                Message::Text(txt) => {
                    let v: Value = serde_json::from_str(txt.as_ref()).expect("client json");
                    let ty = v["type"].as_str().unwrap_or_default().to_string();
                    let ack = serde_json::json!({
                        "type": "response.text.delta",
                        "event_id": format!("srv_ack_{ty}"),
                        "response_id": ty,
                        "output_index": 0,
                        "delta": "ack",
                    });
                    ws.send(Message::Text(ack.to_string())).await.unwrap();
                    if ty == "input_audio_buffer.append" {
                        let audio_delta = serde_json::json!({
                            "type": "response.audio.delta",
                            "event_id": "srv_bin_audio",
                            "response_id": "resp_bin",
                            "output_index": 1,
                            "delta": "QUJDREU=", // base64("ABCDE")
                        });
                        ws.send(Message::binary(audio_delta.to_string().into_bytes()))
                            .await
                            .unwrap();
                    }
                    if ty == "response.cancel" {
                        ws.send(Message::Text(
                            serde_json::json!({
                                "type": "response.cancelled",
                                "event_id": "srv_cancelled",
                                "response_id": "resp_live",
                            })
                            .to_string(),
                        ))
                        .await
                        .unwrap();
                    }
                }
                Message::Close(_) => {
                    // Echo the close frame back for a clean handshake.
                    let _ = ws.send(Message::Close(None)).await;
                    break;
                }
                _ => {}
            }
        }
    }

    fn spawn_echo_server() -> (String, tokio::task::JoinHandle<()>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let addr = listener.local_addr().unwrap();
        let listener = tokio::net::TcpListener::from_std(listener).unwrap();
        let handle = tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let stream = MaybeTlsStream::Plain(stream);
                tokio::spawn(async move {
                    let ws = tokio_tungstenite::accept_async(stream)
                        .await
                        .expect("handshake");
                    run_echo_server(ws).await;
                });
            }
        });
        (format!("ws://{addr}"), handle)
    }

    /// Server that records selected handshake headers then behaves like the
    /// echo server.
    fn spawn_header_capture_server(
        captured: Arc<StdMutex<HashMap<String, String>>>,
    ) -> (String, tokio::task::JoinHandle<()>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let addr = listener.local_addr().unwrap();
        let listener = tokio::net::TcpListener::from_std(listener).unwrap();
        let handle = tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let stream = MaybeTlsStream::Plain(stream);
                let captured = captured.clone();
                tokio::spawn(async move {
                    #[allow(clippy::result_large_err)] // signature fixed by tungstenite's API
                    let callback =
                        |req: &tokio_tungstenite::tungstenite::http::Request<()>,
                         resp: tokio_tungstenite::tungstenite::http::Response<()>| {
                            for name in ["authorization", "x-siren-session"] {
                                if let Some(v) = req.headers().get(name) {
                                    captured.lock().unwrap().insert(
                                        name.to_string(),
                                        String::from_utf8_lossy(v.as_bytes()).into_owned(),
                                    );
                                }
                            }
                            Ok(resp)
                        };
                    if let Ok(ws) = tokio_tungstenite::accept_hdr_async(stream, callback).await {
                        run_echo_server(ws).await;
                    }
                });
            }
        });
        (format!("ws://{addr}"), handle)
    }

    #[tokio::test]
    async fn full_duplex_event_roundtrip_including_binary_frames() {
        let (url, server) = spawn_echo_server();
        let conn = RealtimeConnection::connect(&url, &reqwest::header::HeaderMap::new())
            .await
            .expect("connect");

        // Client -> server -> client text round-trip.
        conn.send(&RealtimeClientEvent::SessionUpdate {
            event_id: "evt_s".into(),
            session: crate::realtime::RealtimeSessionConfig {
                model: Some("siren-realtime-1".into()),
                voice: Some("alloy".into()),
                ..Default::default()
            },
        })
        .await
        .unwrap();
        let ack = conn.recv().await.unwrap().expect("event");
        match ack {
            RealtimeServerEvent::ResponseTextDelta {
                ref response_id,
                ref delta,
                ..
            } => {
                assert_eq!(response_id, "session.update");
                assert_eq!(delta, "ack");
            }
            other => panic!("expected ack delta, got {other:?}"),
        }

        // Audio append: ack arrives as text AND audio delta arrives as a
        // BINARY frame — both must surface as typed server events.
        conn.send(&RealtimeClientEvent::InputAudioBufferAppend {
            event_id: "evt_a".into(),
            audio: "QUJDREU=".into(),
        })
        .await
        .unwrap();

        // Consume via the Stream adapter to exercise it under load.
        let mut events = conn.events();
        let mut got_audio_delta_from_binary = false;
        let mut saw_append_ack = false;
        for _ in 0..2 {
            let ev = events.next().await.unwrap().unwrap();
            match ev {
                RealtimeServerEvent::ResponseAudioDelta {
                    response_id,
                    delta,
                    output_index,
                    ..
                } => {
                    assert_eq!(response_id, "resp_bin");
                    assert_eq!(delta, "QUJDREU=");
                    assert_eq!(output_index, 1);
                    got_audio_delta_from_binary = true;
                }
                RealtimeServerEvent::ResponseTextDelta { response_id, .. } => {
                    assert_eq!(response_id, "input_audio_buffer.append");
                    saw_append_ack = true;
                }
                other => panic!("unexpected event {other:?}"),
            }
        }
        assert!(
            got_audio_delta_from_binary && saw_append_ack,
            "binary-frame audio delta must map through the framer"
        );

        // Interruption path: cancel -> server acks, then emits
        // response.cancelled. Drain until the interruption signal arrives.
        conn.cancel_response("evt_cancel").await.unwrap();
        let mut saw_interruption = false;
        for _ in 0..2 {
            if let Some(ev) = conn.recv().await.unwrap() {
                if ev.is_interruption() {
                    saw_interruption = true;
                    break;
                }
            }
        }
        assert!(saw_interruption, "expected a response.cancelled signal");

        // Graceful close: subsequent recv reports clean shutdown.
        conn.close().await.unwrap();
        assert_eq!(conn.recv().await.unwrap(), None);

        server.abort();
    }

    #[tokio::test]
    async fn connect_applies_custom_handshake_headers() {
        let captured = Arc::new(StdMutex::new(HashMap::new()));
        let (url, server) = spawn_header_capture_server(captured.clone());
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::AUTHORIZATION,
            reqwest::header::HeaderValue::from_static("Bearer siren-secret"),
        );
        headers.insert(
            reqwest::header::HeaderName::from_static("x-siren-session"),
            reqwest::header::HeaderValue::from_static("sess_42"),
        );
        let conn = RealtimeConnection::connect(&url, &headers).await.unwrap();
        conn.send(&RealtimeClientEvent::InputAudioBufferCommit {
            event_id: "evt_c".into(),
        })
        .await
        .unwrap();
        assert!(conn.recv().await.is_ok());

        // Copy out under the lock; do not hold it across awaits below.
        let (auth, session_hdr) = {
            let map = captured.lock().unwrap();
            (
                map.get("authorization").cloned(),
                map.get("x-siren-session").cloned(),
            )
        };
        assert_eq!(auth.as_deref(), Some("Bearer siren-secret"));
        assert_eq!(session_hdr.as_deref(), Some("sess_42"));

        conn.close().await.unwrap();
        server.abort();
    }

    #[tokio::test]
    async fn unknown_server_events_survive_the_wire() {
        // Dedicated server emitting only an unrecognized event type.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let addr = listener.local_addr().unwrap();
        let listener = tokio::net::TcpListener::from_std(listener).unwrap();
        let server = tokio::spawn(async move {
            if let Ok((stream, _)) = listener.accept().await {
                let stream = MaybeTlsStream::Plain(stream);
                if let Ok(mut ws) = tokio_tungstenite::accept_async(stream).await {
                    let _ = ws
                        .send(Message::Text(
                            serde_json::json!({
                                "type": "provider.brand_new_event.v9",
                                "event_id": "evt_future",
                                "opaque": {"a": [1, 2]}
                            })
                            .to_string(),
                        ))
                        .await;
                    let _ = ws.flush().await;
                    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                }
            }
        });

        let url = format!("ws://{addr}");
        let conn = RealtimeConnection::connect(&url, &reqwest::header::HeaderMap::new())
            .await
            .unwrap();
        let ev = conn.recv().await.unwrap().expect("unknown event");
        match ev {
            RealtimeServerEvent::Other { event_id, raw } => {
                assert_eq!(event_id.as_deref(), Some("evt_future"));
                assert_eq!(raw["opaque"]["a"][1], 2);
            }
            other => panic!("expected Other, got {other:?}"),
        }
        server.abort();
    }
}
