//! Full-duplex realtime voice session ([`DuplexSession`]) with barge-in.
//!
//! Lives in `ai-voice` (not `ai-protocols`) because it orchestrates
//! protocol-level transport primitives from `ai-protocols`
//! (`RealtimeConnection`, client/server events) together with voice-domain
//! detectors (`VoiceActivityDetector`, PCM audio) -- voice depends on
//! protocols, never the reverse.
//!
//! Orchestration model: a single owner task drives the session by alternating
//! between [`DuplexSession::send_mic_chunk`] (mic -> `input_audio_buffer.append`,
//! plus local VAD) and [`DuplexSession::process_server_event`] (server events ->
//! jitter-buffered playback / interruption signals), typically under
//! `tokio::select!`. Both directions share one WebSocket through the split,
//! task-safe halves of `RealtimeConnection`.
//!
//! Barge-in triggers (either one cancels playback):
//! 1. **Local VAD**: speech detected in a mic chunk *while* playback is
//!    active -> send `response.cancel`; [`BargeIn::latency_ms`] measures
//!    wall-clock time from speech-detected to cancellation-sent.
//! 2. **Server interruption**: a `response.cancelled` server event arrives ->
//!    drop the jitter buffer immediately.

use std::collections::VecDeque;
use std::time::Instant;

use ai_errors::{AiError, SerializationError};
use ai_protocols::RealtimeClientEvent;
use ai_protocols::RealtimeServerEvent;
use ai_protocols::transport::RealtimeConnection;

use crate::Audio;
use crate::vad::{VadConfig, VadDecision, VoiceActivityDetector};

// ---------------------------------------------------------------------------
// Minimal standard base64 (encode + decode), dependency-free and deterministic.
// ---------------------------------------------------------------------------

const B64_ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Standard base64 with padding (RFC 4648).
pub(crate) fn base64_encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(B64_ALPHABET[(n >> 18 & 63) as usize] as char);
        out.push(B64_ALPHABET[(n >> 12 & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            B64_ALPHABET[(n >> 6 & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            B64_ALPHABET[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

/// Standard base64 decode; rejects invalid characters/padding.
pub(crate) fn base64_decode(encoded: &str) -> Result<Vec<u8>, AiError> {
    fn value(c: u8) -> Option<u32> {
        match c {
            b'A'..=b'Z' => Some((c - b'A') as u32),
            b'a'..=b'z' => Some((c - b'a' + 26) as u32),
            b'0'..=b'9' => Some((c - b'0' + 52) as u32),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }
    let bytes: Vec<u8> = encoded
        .bytes()
        .filter(|b| !b" \n\r\t".contains(b))
        .collect();
    let unpadded_len = bytes.iter().position(|&b| b == b'=').unwrap_or(bytes.len());
    if unpadded_len + 1 < bytes.len() {
        return Err(AiError::Serialization(SerializationError::new(
            "base64: stray data after padding",
        )));
    }
    let mut out = Vec::with_capacity(unpadded_len * 3 / 4);
    for chunk in bytes.chunks(4) {
        if chunk.len() == 1 {
            return Err(AiError::Serialization(SerializationError::new(
                "base64: dangling byte",
            )));
        }
        let mut n = 0u32;
        for (i, &c) in chunk.iter().enumerate() {
            let v = value(c).ok_or_else(|| {
                AiError::Serialization(SerializationError::new(format!(
                    "base64: invalid character {:?}",
                    c as char
                )))
            })?;
            n |= v << (18 - 6 * i);
        }
        out.push((n >> 16) as u8);
        if chunk.len() > 2 {
            out.push((n >> 8) as u8);
        }
        if chunk.len() > 3 {
            out.push(n as u8);
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Jitter buffer for streamed TTS playback chunks.
// ---------------------------------------------------------------------------

/// Orders out-of-arrival playback chunks behind a playout delay so brief
/// network gaps do not stutter playback.
#[derive(Debug)]
pub struct JitterBuffer {
    target_ms: u64,
    chunks: VecDeque<Audio>,
    queued_ms: u64,
}

impl JitterBuffer {
    pub fn new(target_ms: u64) -> Self {
        Self {
            target_ms,
            chunks: VecDeque::new(),
            queued_ms: 0,
        }
    }

    /// Target depth (ms) that must be buffered before playback starts.
    pub fn target_ms(&self) -> u64 {
        self.target_ms
    }

    pub fn push(&mut self, audio: Audio) {
        self.queued_ms += audio.duration_ms;
        self.chunks.push_back(audio);
    }

    /// True once at least the target depth has been buffered.
    pub fn has_playable(&self) -> bool {
        self.queued_ms >= self.target_ms && !self.chunks.is_empty()
    }

    pub fn buffered_ms(&self) -> u64 {
        self.queued_ms
    }

    pub fn len(&self) -> usize {
        self.chunks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.chunks.is_empty()
    }

    /// Pops the next buffered chunk for playout.
    pub fn pop(&mut self) -> Option<Audio> {
        let chunk = self.chunks.pop_front()?;
        self.queued_ms = self.queued_ms.saturating_sub(chunk.duration_ms);
        Some(chunk)
    }

    /// Drops everything (barge-in / flush).
    pub fn clear(&mut self) {
        self.chunks.clear();
        self.queued_ms = 0;
    }
}

// ---------------------------------------------------------------------------
// Duplex session with barge-in.
// ---------------------------------------------------------------------------

/// A completed barge-in with its measured reaction latency.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BargeIn {
    /// Milliseconds from local speech detection to `response.cancel` sent
    /// (rounded down; see [`Self::latency_us`] for sub-millisecond runs).
    pub latency_ms: u64,
    /// Same measurement at microsecond precision.
    pub latency_us: u64,
    /// Whether the trigger was the local VAD (true) or a server signal.
    pub triggered_by_local_vad: bool,
}

/// Notifications surfaced by the receive pump.
#[derive(Debug, Clone, PartialEq)]
pub enum SessionNotification {
    /// The jitter buffer crossed its target depth; playback may start.
    PlaybackReady,
    /// Server confirmed cancellation of the current response.
    InterruptedByServer,
    /// Any other server event, passed through untouched.
    Event(RealtimeServerEvent),
}

/// Configuration for [`DuplexSession`].
#[derive(Debug, Clone)]
pub struct DuplexSessionConfig {
    /// Sample rate of the mic PCM fed to [`DuplexSession::send_mic_chunk`].
    pub input_sample_rate: u32,
    /// Sample rate of the decoded server audio deltas.
    pub output_sample_rate: u32,
    /// Jitter buffer playout target in milliseconds.
    pub jitter_target_ms: u64,
    /// Local VAD configuration (adaptive noise floor recommended).
    pub vad: VadConfig,
}

impl Default for DuplexSessionConfig {
    fn default() -> Self {
        Self {
            input_sample_rate: 16_000,
            output_sample_rate: 24_000,
            jitter_target_ms: 60,
            vad: VadConfig::default(),
        }
    }
}

/// Orchestrates mic upload, jitter-buffered playback, and interruption
/// (barge-in) over one realtime WebSocket connection.
pub struct DuplexSession {
    conn: RealtimeConnection,
    vad: VoiceActivityDetector,
    jitter: JitterBuffer,
    config: DuplexSessionConfig,
    playing: bool,
    next_event_id: u64,
}

impl DuplexSession {
    /// Opens a new realtime connection and wraps it in a session.
    pub async fn connect(
        url: impl AsRef<str>,
        headers: &reqwest::header::HeaderMap,
        config: DuplexSessionConfig,
    ) -> Result<Self, AiError> {
        let conn = RealtimeConnection::connect(url, headers).await?;
        Ok(Self::from_connection(conn, config))
    }

    /// Wraps an existing connection.
    pub fn from_connection(conn: RealtimeConnection, config: DuplexSessionConfig) -> Self {
        let jitter_target = config.jitter_target_ms;
        Self {
            vad: VoiceActivityDetector::new(config.vad.clone()),
            jitter: JitterBuffer::new(jitter_target),
            conn,
            config,
            playing: false,
            next_event_id: 0,
        }
    }

    fn event_id(&mut self) -> String {
        self.next_event_id += 1;
        format!("evt_siren_{}", self.next_event_id)
    }

    /// True while server audio is actively being buffered/played.
    pub fn is_playing(&self) -> bool {
        self.playing
    }

    pub fn buffered_ms(&self) -> u64 {
        self.jitter.buffered_ms()
    }

    /// Sends one mic chunk as `input_audio_buffer.append` (base64 PCM16LE).
    ///
    /// While playback is active, the chunk also runs through the local VAD;
    /// speech detection fires barge-in: playback is cancelled, a
    /// `response.cancel` is written to the wire, and the measured
    /// detection-to-cancellation latency is returned.
    pub async fn send_mic_chunk(&mut self, audio: &Audio) -> Result<Option<BargeIn>, AiError> {
        // Mic upload path (always).
        let pcm = audio.to_pcm_le_bytes();
        let append = RealtimeClientEvent::InputAudioBufferAppend {
            event_id: self.event_id(),
            audio: base64_encode(&pcm),
        };
        self.conn.send(&append).await?;

        // Local barge-in detection path.
        if self.playing {
            let decisions = self.vad.process_frames(&audio.resample(16_000));
            if decisions.contains(&VadDecision::Speech) {
                let detected_at = Instant::now();
                let cancel = RealtimeClientEvent::ResponseCancel {
                    event_id: self.event_id(),
                };
                self.conn.send(&cancel).await?;
                let elapsed = detected_at.elapsed();
                let latency_ms = elapsed.as_millis() as u64;
                let latency_us = elapsed.as_micros() as u64;
                self.cancel_local_playback();
                return Ok(Some(BargeIn {
                    latency_ms,
                    latency_us,
                    triggered_by_local_vad: true,
                }));
            }
        }
        Ok(None)
    }

    /// Commits the pending input audio buffer (end-of-utterance).
    pub async fn commit_input(&mut self) -> Result<(), AiError> {
        let event = RealtimeClientEvent::InputAudioBufferCommit {
            event_id: self.event_id(),
        };
        self.conn.send(&event).await
    }

    /// Requests a new server response (start talking).
    pub async fn request_response(&mut self) -> Result<(), AiError> {
        let event = RealtimeClientEvent::ResponseCreate {
            event_id: self.event_id(),
            response: None,
        };
        self.conn.send(&event).await
    }

    /// Pulls ONE server event off the wire and folds it into session state.
    ///
    /// `response.audio.delta` payloads are base64-decoded into PCM and
    /// pushed into the jitter buffer; when the playout target is reached the
    /// session flips to playing and emits [`SessionNotification::PlaybackReady`].
    /// Interruption signals clear playback instantly.
    pub async fn process_server_event(&mut self) -> Result<Option<SessionNotification>, AiError> {
        match self.conn.recv().await? {
            None => Ok(None),
            Some(event) => match event {
                RealtimeServerEvent::ResponseAudioDelta { delta, .. } => {
                    let pcm = base64_decode(&delta)?;
                    let samples: Vec<i16> = pcm
                        .chunks_exact(2)
                        .map(|p| i16::from_le_bytes([p[0], p[1]]))
                        .collect();
                    self.jitter
                        .push(Audio::from_samples(samples, self.config.output_sample_rate));
                    if !self.playing && self.jitter.has_playable() {
                        self.playing = true;
                        return Ok(Some(SessionNotification::PlaybackReady));
                    }
                    Ok(None)
                }
                e if e.is_interruption() => {
                    self.cancel_local_playback();
                    Ok(Some(SessionNotification::InterruptedByServer))
                }
                other => Ok(Some(SessionNotification::Event(other))),
            },
        }
    }

    /// Drains the current playback queue (returns chunks in order).
    pub fn take_playback(&mut self) -> Vec<Audio> {
        std::iter::from_fn(|| self.jitter.pop()).collect()
    }

    fn cancel_local_playback(&mut self) {
        self.jitter.clear();
        self.playing = false;
        self.vad.reset();
    }

    /// Gracefully closes the underlying connection.
    pub async fn close(self) -> Result<(), AiError> {
        self.conn.close().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_roundtrips_known_vectors() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64_decode("Zm9vYmFy").unwrap(), b"foobar");
        assert_eq!(
            base64_decode(&base64_encode(&[7u8; 999])).unwrap(),
            vec![7u8; 999]
        );
        assert!(base64_decode("a*bc").is_err());
    }

    #[test]
    fn jitter_buffer_gates_playback_behind_target_depth() {
        let mut jb = JitterBuffer::new(100);
        assert!(!jb.has_playable());
        jb.push(Audio::from_samples(vec![0i16; 480], 16_000)); // 30ms
        assert_eq!(jb.buffered_ms(), 30);
        jb.push(Audio::from_samples(vec![0i16; 480], 16_000)); // 60ms
        assert!(!jb.has_playable());
        jb.push(Audio::from_samples(vec![0i16; 800], 16_000)); // +50ms
        assert_eq!(jb.buffered_ms(), 110);
        assert!(jb.has_playable());
        assert_eq!(jb.pop().map(|a| a.samples.len()), Some(480));
        assert_eq!(jb.buffered_ms(), 80);
        jb.clear();
        assert!(jb.is_empty());
        assert!(!jb.has_playable());
    }

    // ------------------------------------------------------------------
    // Loopback integration: simulated realtime server + barge-in proof.
    // ------------------------------------------------------------------
    mod loopback {
        use super::*;
        use futures::{SinkExt, StreamExt};
        use tokio::net::TcpStream;
        use tokio_tungstenite::tungstenite::Message;
        use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

        const OUTPUT_RATE: u32 = 24_000;

        /// Simulated realtime server: acknowledges appends and streams
        /// continuous TTS-sized audio deltas (~10ms each) until closed.
        ///
        /// The socket is split: one task pushes audio deltas on a timer
        /// (playback must stream regardless of client traffic), the main
        /// loop answers client events.
        async fn run_streaming_server(ws: WebSocketStream<MaybeTlsStream<TcpStream>>) {
            let created = serde_json::json!({
                "type": "session.created", "event_id": "srv_1", "session": {}
            });
            let (mut sink, mut source) = ws.split();
            sink.send(Message::Text(created.to_string())).await.ok();

            // One ~10ms chunk of low-level tone at 24 kHz.
            let chunk_samples: Vec<i16> = (0..240).map(|i| (i % 40 * 20) as i16).collect();
            let pcm: Vec<u8> = chunk_samples.iter().flat_map(|s| s.to_le_bytes()).collect();
            let audio_b64 = base64_encode(&pcm);
            let delta = serde_json::json!({
                "type": "response.audio.delta",
                "event_id": "srv_audio", "response_id": "resp_stream",
                "output_index": 0, "delta": audio_b64,
            })
            .to_string();

            let (tx, mut rx) = tokio::sync::mpsc::channel::<Message>(8);

            // Writer task: streams audio deltas on a timer AND any control
            // replies forwarded by the reader loop.
            let writer = tokio::spawn(async move {
                loop {
                    tokio::select! {
                        send = sink.send(Message::Text(delta.clone())) => {
                            if send.is_err() {
                                break;
                            }
                            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
                        }
                        reply = rx.recv() => {
                            match reply {
                                Some(msg) => { if sink.send(msg).await.is_err() { break; } }
                                None => break,
                            }
                        }
                    }
                }
            });

            while let Some(msg) = source.next().await {
                match msg.expect("streaming server error") {
                    Message::Text(txt) => {
                        let v: serde_json::Value =
                            serde_json::from_str(txt.as_ref()).expect("client json");
                        match v["type"].as_str().unwrap_or_default() {
                            "input_audio_buffer.append" => {
                                let ack = serde_json::json!({
                                    "type": "response.text.delta",
                                    "event_id": "srv_ack", "response_id": "resp_stream",
                                    "output_index": 0, "delta": "."
                                })
                                .to_string();
                                tx.send(Message::Text(ack)).await.ok();
                            }
                            "response.cancel" => {
                                let cancelled = serde_json::json!({
                                    "type": "response.cancelled",
                                    "event_id": "srv_cancelled", "response_id": "resp_stream",
                                })
                                .to_string();
                                tx.send(Message::Text(cancelled)).await.ok();
                            }
                            _ => {}
                        }
                    }
                    Message::Close(_) => break,
                    _ => {}
                }
            }
            writer.abort();
        }

        fn spawn_server() -> (String, tokio::task::JoinHandle<()>) {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            listener.set_nonblocking(true).unwrap();
            let addr = listener.local_addr().unwrap();
            let listener = tokio::net::TcpListener::from_std(listener).unwrap();
            let handle = tokio::spawn(async move {
                while let Ok((stream, _)) = listener.accept().await {
                    let stream = MaybeTlsStream::Plain(stream);
                    tokio::spawn(async move {
                        if let Ok(ws) = tokio_tungstenite::accept_async(stream).await {
                            run_streaming_server(ws).await;
                        }
                    });
                }
            });
            (format!("ws://{addr}"), handle)
        }

        fn session_config() -> DuplexSessionConfig {
            DuplexSessionConfig {
                output_sample_rate: OUTPUT_RATE,
                input_sample_rate: 16_000,
                // 3 x ~10ms deltas -> quick, deterministic playback start.
                jitter_target_ms: 25,
                vad: VadConfig {
                    rms_threshold: 500.0,
                    hangover_frames: 2,
                    ..Default::default()
                },
            }
        }

        fn silence_chunk(ms: u32) -> Audio {
            let n = (16_000u64 * ms as u64 / 1000) as usize;
            Audio::from_samples(vec![0i16; n], 16_000)
        }

        fn speech_chunk(ms: u32) -> Audio {
            let n = (16_000u64 * ms as u64 / 1000) as usize;
            Audio::from_samples(vec![3000i16; n], 16_000)
        }

        /// PROOF: server streams TTS audio continuously; speech-shaped audio
        /// fed mid-playback fires barge-in with measured latency well under
        /// the 300ms bound (local work only: detect -> serialize -> WS send).
        #[tokio::test]
        async fn speech_mid_playback_triggers_barge_in_under_300ms() {
            let (url, server) = spawn_server();
            let mut session =
                DuplexSession::connect(&url, &reqwest::header::HeaderMap::new(), session_config())
                    .await
                    .expect("session connect");

            // Phase 1: pump server events until playback starts.
            let playing_deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
            while !session.is_playing() {
                assert!(
                    tokio::time::timeout_at(playing_deadline, session.process_server_event())
                        .await
                        .expect("timed out waiting for playback")
                        .is_ok(),
                    "receive error before playback"
                );
            }
            assert!(session.buffered_ms() >= 25);

            // Phase 2: user starts speaking mid-playback -> barge-in.
            let mut barge_in = None;
            for _ in 0..5 {
                if let Some(event) = session.send_mic_chunk(&speech_chunk(40)).await.unwrap() {
                    barge_in = Some(event);
                    break;
                }
            }
            let barge_in = barge_in.expect("barge-in should fire while server audio plays");
            println!(
                "SIREN-BARGE-IN-EVIDENCE: fired=true latency_ms={} latency_us={} bound_ms=300 triggered_by_local_vad={}",
                barge_in.latency_ms, barge_in.latency_us, barge_in.triggered_by_local_vad
            );
            assert!(
                barge_in.latency_ms < 300,
                "barge-in latency {}ms exceeded the 300ms bound",
                barge_in.latency_ms
            );
            assert!(barge_in.triggered_by_local_vad);

            // Playback was cancelled: jitter buffer emptied, not playing.
            assert!(!session.is_playing());
            assert_eq!(session.buffered_ms(), 0);

            // Quiet audio no longer re-triggers anything (no playback active).
            assert!(
                session
                    .send_mic_chunk(&silence_chunk(40))
                    .await
                    .unwrap()
                    .is_none()
            );

            session.close().await.ok();
            server.abort();
        }

        /// Server-driven interruption: a `response.cancelled` signal drops
        /// playback even without local speech.
        #[tokio::test]
        async fn server_cancellation_signal_clears_playback() {
            let (url, server) = spawn_server();
            let mut session =
                DuplexSession::connect(&url, &reqwest::header::HeaderMap::new(), session_config())
                    .await
                    .expect("session connect");

            // Pump until playing.
            let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
            while !session.is_playing() {
                tokio::time::timeout_at(deadline, session.process_server_event())
                    .await
                    .expect("timed out waiting for playback")
                    .unwrap();
            }

            // Ask the (simulated) provider to cancel its own response.
            session
                .conn
                .send(&RealtimeClientEvent::ResponseCancel {
                    event_id: "evt_ext_cancel".into(),
                })
                .await
                .unwrap();

            let mut interrupted = false;
            let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
            while tokio::time::Instant::now() < deadline {
                match tokio::time::timeout(
                    std::time::Duration::from_secs(1),
                    session.process_server_event(),
                )
                .await
                .expect("recv timed out")
                .unwrap()
                {
                    Some(SessionNotification::InterruptedByServer) => {
                        interrupted = true;
                        break;
                    }
                    _ => continue,
                }
            }
            assert!(interrupted, "expected InterruptedByServer notification");
            assert!(!session.is_playing());

            session.close().await.ok();
            server.abort();
        }
    }
}
