//! In-crate deterministic chaos server: a minimal HTTP/1.1 endpoint with
//! scriptable fault injection, used to *prove* resilience behavior under
//! injected failures instead of asserting happy paths against mocks.
//!
//! The server speaks just enough HTTP/1.1 for `reqwest`-backed clients:
//! it reads one request (headers + `Content-Length` body), consults a
//! **seeded** random schedule once per request, and answers according to the
//! configured fault mix:
//!
//! | knob                    | injected failure                                  |
//! |-------------------------|---------------------------------------------------|
//! | `drop_connection_pct`   | connection closed before any bytes are written     |
//! | `stall_past_deadline_pct` | response held past any sane client deadline      |
//! | `http_500_pct`          | HTTP 500 (provider 5xx ⇒ retryable)                |
//! | `http_429_pct`          | HTTP 429 + `Retry-After` (rate limit ⇒ retryable)  |
//! | `garbage_body_pct`      | HTTP 200 with an unparseable body                  |
//! | *(remainder)*           | healthy OpenAI-style completion JSON               |
//!
//! One uniform roll per request is compared against cumulative bands in a
//! fixed knob order, so a run is fully reproducible given the seed and the
//! request interleaving. Knobs are atomics: tests may retune the fault mix
//! mid-run (e.g. to heal the dependency while probing circuit recovery).
//!
//! Server-side counters ([`ChaosServer::metrics`]) record how often each
//! fault fired plus the maximum number of requests observed concurrently —
//! letting tests verify concurrency limits *from the server's point of view*
//! rather than trusting client-side bookkeeping.

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::time::Duration;

use parking_lot::Mutex;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Notify;

/// Hard caps so a misbehaving peer cannot grow buffers unboundedly.
const MAX_HEADER_BYTES: usize = 64 * 1024;
const MAX_BODY_BYTES: usize = 10 * 1024 * 1024;
const READ_TIMEOUT: Duration = Duration::from_secs(5);

/// Fault-injection knobs (percentages are `0..=100`). All fields are atomics
/// so tests can retune the mix while the server runs.
#[derive(Debug)]
pub struct ChaosKnobs {
    /// Seed driving the deterministic fault schedule.
    pub seed: u64,
    /// Latency added to healthy responses (ms).
    pub healthy_latency_ms: AtomicU64,
    /// % of requests whose connection is dropped without a response.
    pub drop_connection_pct: AtomicU64,
    /// % of requests stalled past any reasonable client deadline.
    pub stall_past_deadline_pct: AtomicU64,
    /// How long stalled requests sleep before responding anyway (ms).
    pub stall_ms: AtomicU64,
    /// % of requests answered with HTTP 500.
    pub http_500_pct: AtomicU64,
    /// % of requests answered with HTTP 429 (+ `Retry-After: 0`).
    pub http_429_pct: AtomicU64,
    /// % of requests answered with HTTP 200 and an unparseable body.
    pub garbage_body_pct: AtomicU64,
}

impl ChaosKnobs {
    /// A fully healthy server with the given base latency and seed.
    pub fn healthy(seed: u64) -> Self {
        Self {
            seed,
            healthy_latency_ms: AtomicU64::new(0),
            drop_connection_pct: AtomicU64::new(0),
            stall_past_deadline_pct: AtomicU64::new(0),
            stall_ms: AtomicU64::new(0),
            http_500_pct: AtomicU64::new(0),
            http_429_pct: AtomicU64::new(0),
            garbage_body_pct: AtomicU64::new(0),
        }
    }

    fn load(store: &AtomicU64) -> u64 {
        store.load(Ordering::Relaxed).min(100)
    }

    /// Total percentage of requests that receive *some* fault.
    pub fn total_fault_pct(&self) -> u64 {
        Self::load(&self.drop_connection_pct)
            + Self::load(&self.stall_past_deadline_pct)
            + Self::load(&self.http_500_pct)
            + Self::load(&self.http_429_pct)
            + Self::load(&self.garbage_body_pct)
    }
}

/// Immutable snapshot of the server-side counters.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ChaosSnapshot {
    pub accepted_connections: u64,
    pub requests_served: u64,
    pub connections_dropped: u64,
    pub stalls: u64,
    pub responses_500: u64,
    pub responses_429: u64,
    pub garbage_bodies: u64,
    pub healthy_responses: u64,
    /// High-water mark of concurrently-handled requests (server-observed).
    pub max_in_flight: u64,
}

#[derive(Debug, Default)]
struct ChaosCounters {
    accepted_connections: AtomicU64,
    requests_served: AtomicU64,
    connections_dropped: AtomicU64,
    stalls: AtomicU64,
    responses_500: AtomicU64,
    responses_429: AtomicU64,
    garbage_bodies: AtomicU64,
    healthy_responses: AtomicU64,
    in_flight: AtomicI64,
    max_in_flight: AtomicU64,
}

impl ChaosCounters {
    fn snapshot(&self) -> ChaosSnapshot {
        ChaosSnapshot {
            accepted_connections: self.accepted_connections.load(Ordering::Relaxed),
            requests_served: self.requests_served.load(Ordering::Relaxed),
            connections_dropped: self.connections_dropped.load(Ordering::Relaxed),
            stalls: self.stalls.load(Ordering::Relaxed),
            responses_500: self.responses_500.load(Ordering::Relaxed),
            responses_429: self.responses_429.load(Ordering::Relaxed),
            garbage_bodies: self.garbage_bodies.load(Ordering::Relaxed),
            healthy_responses: self.healthy_responses.load(Ordering::Relaxed),
            max_in_flight: self.max_in_flight.load(Ordering::Relaxed),
        }
    }

    fn enter(&self) {
        let now = self.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
        self.max_in_flight
            .fetch_max(now.max(0) as u64, Ordering::SeqCst);
    }

    fn leave(&self) {
        self.in_flight.fetch_sub(1, Ordering::SeqCst);
    }
}

/// A running chaos server bound to `127.0.0.1` on an OS-assigned port.
pub struct ChaosServer {
    addr: SocketAddr,
    knobs: Arc<ChaosKnobs>,
    counters: Arc<ChaosCounters>,
    shutdown: Arc<Notify>,
    accept_task: tokio::task::JoinHandle<()>,
}

impl ChaosServer {
    /// Binds and starts serving. Must be awaited within a tokio runtime.
    pub async fn start(knobs: ChaosKnobs) -> std::io::Result<Self> {
        let listener = TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0))).await?;
        let addr = listener.local_addr()?;
        let knobs = Arc::new(knobs);
        let counters = Arc::new(ChaosCounters::default());
        let shutdown = Arc::new(Notify::new());

        let accept_task = tokio::spawn(run_accept(
            listener,
            Arc::clone(&knobs),
            Arc::clone(&counters),
            Arc::clone(&shutdown),
        ));

        Ok(Self {
            addr,
            knobs,
            counters,
            shutdown,
            accept_task,
        })
    }

    /// Base URL for HTTP clients (`http://127.0.0.1:<port>`).
    pub fn url(&self) -> String {
        format!("http://{}", self.addr)
    }

    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    pub fn knobs(&self) -> &ChaosKnobs {
        &self.knobs
    }

    /// Current values of all server-side counters.
    pub fn metrics(&self) -> ChaosSnapshot {
        self.counters.snapshot()
    }

    /// Stops accepting new connections and waits for the accept task to end.
    /// Already-open short-lived connections drain on their own.
    pub async fn shutdown(self) {
        self.shutdown.notify_waiters();
        let _ = self.accept_task.await;
    }
}

async fn run_accept(
    listener: TcpListener,
    knobs: Arc<ChaosKnobs>,
    counters: Arc<ChaosCounters>,
    shutdown: Arc<Notify>,
) {
    let rng = Arc::new(Mutex::new(StdRng::seed_from_u64(knobs.seed)));
    loop {
        let socket = tokio::select! {
            _ = shutdown.notified() => break,
            accepted = listener.accept() => match accepted {
                Ok((socket, _peer)) => socket,
                Err(_) => continue,
            },
        };
        counters
            .accepted_connections
            .fetch_add(1, Ordering::Relaxed);
        tokio::spawn(handle_connection(
            socket,
            Arc::clone(&knobs),
            Arc::clone(&counters),
            Arc::clone(&rng),
        ));
    }
}

/// The scripted fault schedule: one roll, compared against cumulative bands
/// in a fixed order (drop → stall → 500 → 429 → garbage → healthy).
#[derive(Debug, PartialEq, Eq)]
enum Verdict {
    DropConnection,
    StallPastDeadline,
    Http500,
    Http429,
    GarbageBody,
    Healthy,
}

fn decide(rng: &mut StdRng, knobs: &ChaosKnobs) -> Verdict {
    let roll: u64 = rng.gen_range(0..100);

    // Cumulative bands in a fixed knob order — the Nth fault fires when the
    // roll lands below the running total of the first N percentages.
    let drop_band = ChaosKnobs::load(&knobs.drop_connection_pct);
    if roll < drop_band {
        return Verdict::DropConnection;
    }
    let stall_band = drop_band + ChaosKnobs::load(&knobs.stall_past_deadline_pct);
    if roll < stall_band {
        return Verdict::StallPastDeadline;
    }
    let server_error_band = stall_band + ChaosKnobs::load(&knobs.http_500_pct);
    if roll < server_error_band {
        return Verdict::Http500;
    }
    let rate_limit_band = server_error_band + ChaosKnobs::load(&knobs.http_429_pct);
    if roll < rate_limit_band {
        return Verdict::Http429;
    }
    if roll < rate_limit_band + ChaosKnobs::load(&knobs.garbage_body_pct) {
        return Verdict::GarbageBody;
    }
    Verdict::Healthy
}

async fn handle_connection(
    mut socket: TcpStream,
    knobs: Arc<ChaosKnobs>,
    counters: Arc<ChaosCounters>,
    rng: Arc<Mutex<StdRng>>,
) {
    // Read exactly one request head + body. The scripted handler ignores
    // method/path semantics entirely; only framing matters.
    if read_request(&mut socket).await.is_none() {
        return; // malformed / timed-out peer: nothing to answer
    }
    counters.requests_served.fetch_add(1, Ordering::Relaxed);
    counters.enter();

    let verdict = {
        let mut rng = rng.lock();
        decide(&mut rng, &knobs)
    };

    let outcome: std::io::Result<()> = match verdict {
        Verdict::DropConnection => {
            counters.connections_dropped.fetch_add(1, Ordering::Relaxed);
            // Drop the socket with no bytes written.
            counters.leave();
            return;
        }
        Verdict::StallPastDeadline => {
            counters.stalls.fetch_add(1, Ordering::Relaxed);
            let stall = Duration::from_millis(ChaosKnobs::load(&knobs.stall_ms));
            tokio::time::sleep(stall).await;
            // Answer late regardless; the original caller has almost certainly
            // already timed out and this exercises server-side cleanup.
            respond(&mut socket, ok_response(b"late")).await
        }
        Verdict::Http500 => {
            counters.responses_500.fetch_add(1, Ordering::Relaxed);
            let body = br#"{"error":{"message":"internal chaos","type":"server_error"}}"#;
            respond(
                &mut socket,
                http_response("HTTP/1.1 500 Internal Server Error", "", body),
            )
            .await
        }
        Verdict::Http429 => {
            counters.responses_429.fetch_add(1, Ordering::Relaxed);
            let body = br#"{"error":{"message":"slow down","type":"rate_limit"}}"#;
            respond(
                &mut socket,
                http_response("HTTP/1.1 429 Too Many Requests", "Retry-After: 0\r\n", body),
            )
            .await
        }
        Verdict::GarbageBody => {
            counters.garbage_bodies.fetch_add(1, Ordering::Relaxed);
            respond(&mut socket, garbage_response()).await
        }
        Verdict::Healthy => {
            counters.healthy_responses.fetch_add(1, Ordering::Relaxed);
            let latency = Duration::from_millis(ChaosKnobs::load(&knobs.healthy_latency_ms));
            if !latency.is_zero() {
                tokio::time::sleep(latency).await;
            }
            let seq = counters.healthy_responses.load(Ordering::Relaxed);
            respond(&mut socket, ok_response(ok_body(seq).as_bytes())).await
        }
    };

    counters.leave();
    let _ = outcome; // peer vanishing mid-write (drops/stalls) is expected
}

fn ok_body(seq: u64) -> String {
    format!(
        concat!(
            r#"{{"id":"chatcmpl-chaos-{seq}","object":"chat.completion","#,
            r#""model":"chaos-mini","#,
            r#""choices":[{{"index":0,"message":{{"role":"assistant","content":"pong #{seq}"}},"finish_reason":"stop"}}],"#,
            r#""usage":{{"prompt_tokens":3,"completion_tokens":2,"total_tokens":5}}}}"#
        ),
        seq = seq
    )
}

fn ok_response(body: &[u8]) -> Vec<u8> {
    http_response("HTTP/1.1 200 OK", "", body)
}

fn garbage_response() -> Vec<u8> {
    http_response(
        "HTTP/1.1 200 OK",
        "Content-Type: text/plain\r\n",
        b"<<<< this is absolutely not JSON {{{",
    )
}

fn http_response(status: &str, extra_headers: &str, body: &[u8]) -> Vec<u8> {
    let mut response = Vec::with_capacity(body.len() + 128);
    response.extend_from_slice(
        format!(
            "{status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{extra_headers}Connection: close\r\n\r\n",
            body.len()
        )
        .as_bytes(),
    );
    response.extend_from_slice(body);
    response
}

async fn respond(socket: &mut TcpStream, response: Vec<u8>) -> std::io::Result<()> {
    socket.write_all(&response).await?;
    socket.flush().await?;
    // Half-close so clients observe a complete response immediately even
    // though we advertise `Connection: close`.
    let _ = socket.shutdown().await;
    Ok(())
}

/// Reads one HTTP/1.1 request (head + `Content-Length` framed body).
/// Returns `None` on malformed input, EOF before headers completed, or the
/// read timeout.
async fn read_request(socket: &mut TcpStream) -> Option<()> {
    let mut buffer = Vec::with_capacity(1024);
    let mut chunk = [0u8; 4096];

    let head_end = loop {
        if let Some(pos) = find_header_end(&buffer) {
            break pos;
        }
        if buffer.len() > MAX_HEADER_BYTES {
            return None;
        }
        match tokio::time::timeout(READ_TIMEOUT, socket.read(&mut chunk)).await {
            Ok(Ok(read)) if read > 0 => buffer.extend_from_slice(&chunk[..read]),
            _ => return None, // EOF, io error, or read timeout before a full head
        }
    };

    // The body may have arrived together with the head (small requests are
    // written in one segment): consume whatever is already buffered before
    // touching the socket again.
    let content_length = parse_content_length(&buffer[..head_end]).unwrap_or(0);
    if content_length > MAX_BODY_BYTES {
        return None;
    }
    let mut body_remaining = content_length.saturating_sub(buffer.len().saturating_sub(head_end));
    while body_remaining > 0 {
        let read = tokio::time::timeout(READ_TIMEOUT, socket.read(&mut chunk))
            .await
            .ok()?
            .ok()?;
        if read == 0 {
            return None;
        }
        body_remaining = body_remaining.saturating_sub(read);
    }
    Some(())
}

fn find_header_end(buffer: &[u8]) -> Option<usize> {
    buffer
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .map(|pos| pos + 4)
}

fn parse_content_length(head: &[u8]) -> Option<usize> {
    let head = std::str::from_utf8(head).ok()?;
    for line in head.lines() {
        if let Some((name, value)) = line.split_once(':') {
            if name.eq_ignore_ascii_case("content-length") {
                return value.trim().parse::<usize>().ok();
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Unit tests: determinism of the schedule and HTTP framing helpers.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn healthy_knobs_yield_only_healthy_verdicts() {
        let knobs = ChaosKnobs::healthy(42);
        let mut rng = StdRng::seed_from_u64(knobs.seed);
        for _ in 0..1000 {
            assert_eq!(decide(&mut rng, &knobs), Verdict::Healthy);
        }
    }

    #[test]
    fn schedule_is_reproducible_for_a_seed() {
        let mut k = ChaosKnobs::healthy(7);
        k.http_500_pct = AtomicU64::new(30);
        k.drop_connection_pct = AtomicU64::new(20);

        let run = |seed_offset: u64| {
            let mut local = ChaosKnobs::healthy(k.seed + seed_offset);
            local.http_500_pct = AtomicU64::new(30);
            local.drop_connection_pct = AtomicU64::new(20);
            let mut rng = StdRng::seed_from_u64(local.seed);
            (0..500)
                .map(|_| decide(&mut rng, &local))
                .collect::<Vec<_>>()
        };
        assert_eq!(run(0), run(0), "same seed must replay the same schedule");
        assert_ne!(run(0), run(1), "different seeds should diverge");
    }

    #[test]
    fn bands_match_knob_percentages() {
        let mut knobs = ChaosKnobs::healthy(11);
        knobs.drop_connection_pct = AtomicU64::new(25);
        knobs.http_429_pct = AtomicU64::new(75);

        let mut rng = StdRng::seed_from_u64(knobs.seed);
        let mut drops = 0;
        let mut limits = 0;
        for _ in 0..2000 {
            match decide(&mut rng, &knobs) {
                Verdict::DropConnection => drops += 1,
                Verdict::Http429 => limits += 1,
                other => panic!("unexpected verdict {other:?}"),
            }
        }
        // Statistical sanity around the configured bands (±5pp over 2000 rolls).
        assert!((450..=550).contains(&drops), "drops were {drops}");
        assert!((1450..=1550).contains(&limits), "429s were {limits}");
    }

    #[test]
    fn http_framing_helpers_are_well_formed() {
        let response = http_response("HTTP/1.1 418 I'm a teapot", "X-Test: 1\r\n", b"hi");
        let text = String::from_utf8(response).unwrap();
        assert!(text.starts_with("HTTP/1.1 418"));
        assert!(text.contains("Content-Length: 2"));
        assert!(text.contains("X-Test: 1\r\n"));
        assert!(text.ends_with("\r\n\r\nhi"));

        assert_eq!(
            find_header_end(b"GET /\r\n\r\nbody"),
            Some(9),
            "4-byte terminator found at offset 5"
        );
        assert_eq!(find_header_end(b"GET /\r\n"), None);

        let head = b"POST /v1/chat/completions HTTP/1.1\r\nHost: x\r\nContent-Length: 12\r\n\r\n";
        assert_eq!(parse_content_length(head), Some(12));
        assert_eq!(parse_content_length(b"POST / HTTP/1.1\r\n\r\n"), None);
    }

    #[test]
    fn total_fault_pct_caps_at_each_knob() {
        let mut knobs = ChaosKnobs::healthy(1);
        knobs.drop_connection_pct = AtomicU64::new(150); // >100 clamps
        knobs.http_500_pct = AtomicU64::new(10);
        assert_eq!(knobs.total_fault_pct(), 110);
    }
}
