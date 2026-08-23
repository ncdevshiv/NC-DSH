"""Centralized daemon: HTTP server exposing JSON-RPC + REST + SSE.

Started by ``dataworm init`` or auto-spawned on the first CLI call via
``ensure_daemon()``. Holds a warm ``Core`` (graph + event bus) in memory and
serves every op through the single ``Core.call`` dispatcher.

Transports (all on one ``127.0.0.1:<port>`` HTTP server):
  POST /rpc            JSON-RPC 2.0  -> Core.call(method, params)
  GET  /api/<method>   REST wrapper -> Core.call(method, query params);
                       read-only methods only (READ_ONLY_METHODS) — mutators
                       answer 405 and must use POST /rpc
  GET  /events         SSE live stream of EventBus events (replay + live)
  GET  /               dashboard (built webapp/dist bundle; legacy inline
                       page from live.py as automatic fallback)

Lifecycle:
  - ``ensure_daemon(db_path)`` reads ``.dataworm/daemon.json`` (pid, port, token);
    pings ``/api/ping`` to confirm liveness; if dead/missing, spawns
    ``python -m dataworm.daemon_runner`` detached (Windows-safe), polls the port,
    writes the port-file. Returns a ``DaemonHandle``.
  - ``stop_daemon()`` POSTs ``shutdown`` and removes the port-file.
  - Every CLI->daemon call sends ``Authorization: Bearer <token>``.

The token is a guard against other local processes talking to the port, not a
full security model — the daemon binds to loopback only.
"""

from __future__ import annotations

import http.server
import json
import logging
import os
import secrets
import socket
import socketserver
import subprocess
import sys
import threading
import time
import urllib.parse
from pathlib import Path
from typing import Any

from dataworm.core import Core, DEFAULT_DB
from dataworm.events import EventBus

log = logging.getLogger("dataworm.server")

DEFAULT_PORT = 8765
DAEMON_DIR = ".dataworm"
DAEMON_FILE = "daemon.json"
_START_TIMEOUT = 8.0  # seconds to wait for a spawned daemon to answer

# The methods GET /api/<method> may execute: pure reads the dashboard can
# issue freely. Everything else in Core._METHODS (crawl, watch/unwatch,
# hash_pass, extract_refs, plan_edit, configure_webhook, shutdown, ...)
# mutates state or triggers work, so it is POST /rpc only — a GET answers
# 405 so no crawler, prefetcher or mistyped link can start work as a side
# effect of an innocent fetch.
READ_ONLY_METHODS = {
    "ping", "summary", "roots", "search", "context",
    "impact", "neighbors", "changes", "watched", "graph",
}

# Built dashboard (SolidJS + Vite output), committed into the package so end
# users need zero Node tooling. Served by _RPCHandler._dashboard_html() /
# ._serve_dist_asset(); when absent the legacy inline page is served instead.
DIST_DIR = Path(__file__).resolve().parent / "webapp" / "dist"
_ASSET_TYPES = {
    ".js": "text/javascript",
    ".mjs": "text/javascript",
    ".css": "text/css",
    ".svg": "image/svg+xml",
    ".png": "image/png",
    ".ico": "image/x-icon",
    ".woff2": "font/woff2",
    ".map": "application/json",
}


# ---- port-file helpers ----------------------------------------------------

def _daemon_path(db_path: str = DEFAULT_DB) -> Path:
    """The port-file lives next to the graph DB, in .dataworm/."""
    db = Path(db_path)
    base = db.parent if db.parent != Path(".") else Path(DAEMON_DIR)
    base.mkdir(parents=True, exist_ok=True)
    return base / DAEMON_FILE


def _read_port_file(db_path: str = DEFAULT_DB) -> dict[str, Any] | None:
    p = _daemon_path(db_path)
    if not p.exists():
        return None
    try:
        return json.loads(p.read_text(encoding="utf-8"))
    except Exception:
        return None


def _write_port_file(db_path: str, pid: int, port: int, token: str) -> None:
    p = _daemon_path(db_path)
    p.write_text(json.dumps({
        "pid": pid,
        "port": port,
        "token": token,
        "started": time.strftime("%Y-%m-%dT%H:%M:%S"),
        "db": str(db_path),
    }, indent=2), encoding="utf-8")


def _remove_port_file(db_path: str = DEFAULT_DB) -> None:
    p = _daemon_path(db_path)
    try:
        p.unlink(missing_ok=True)
    except Exception:
        pass


# ---- free port selection --------------------------------------------------

def _free_port(preferred: int = DEFAULT_PORT) -> int:
    """Return a free TCP port on loopback, preferring ``preferred``."""
    for port in (preferred, 0):
        try:
            with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
                s.bind(("127.0.0.1", port))
                return s.getsockname()[1]
        except OSError:
            continue
    # last resort: let the OS pick
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


# ---- HTTP client to the daemon (used by the CLI) --------------------------

def _rpc_call(handle: "DaemonHandle", method: str, params: dict | None = None,
              timeout: float = 30.0) -> dict:
    """POST a JSON-RPC request to the daemon. Returns the result dict."""
    import urllib.request
    payload = json.dumps({
        "jsonrpc": "2.0",
        "method": method,
        "params": params or {},
        "id": 1,
    }).encode("utf-8")
    url = f"http://127.0.0.1:{handle.port}/rpc"
    req = urllib.request.Request(url, data=payload, method="POST",
                                 headers={
                                     "Content-Type": "application/json",
                                     "Authorization": f"Bearer {handle.token}",
                                 })
    with urllib.request.urlopen(req, timeout=timeout) as resp:
        data = json.loads(resp.read().decode("utf-8"))
    if "error" in data:
        return data["error"] if isinstance(data["error"], dict) else {"error": data["error"]}
    return data.get("result", {})


def _is_alive(port: int, token: str) -> bool:
    """Ping the daemon to confirm it's actually our process and responsive."""
    import urllib.request
    try:
        url = f"http://127.0.0.1:{port}/api/ping"
        req = urllib.request.Request(url, headers={"Authorization": f"Bearer {token}"})
        with urllib.request.urlopen(req, timeout=2.0) as resp:
            data = json.loads(resp.read().decode("utf-8"))
            return data.get("ok") is True
    except Exception:
        return False


# ---- spawn-or-reuse -------------------------------------------------------

class DaemonHandle:
    """A handle to a running (or just-spawned) daemon."""
    def __init__(self, pid: int, port: int, token: str, db_path: str) -> None:
        self.pid = pid
        self.port = port
        self.token = token
        self.db_path = db_path

    def call(self, method: str, params: dict | None = None, timeout: float = 30.0) -> dict:
        return _rpc_call(self, method, params, timeout)


def ensure_daemon(db_path: str = DEFAULT_DB, force_start: bool = False,
                  prefer_rust: bool = True) -> DaemonHandle:
    """Return a handle to a running daemon, spawning one if needed.

    1. Read the port-file. If present and the daemon answers a ping, reuse it.
    2. Otherwise spawn ``python -m dataworm.daemon_runner`` detached, poll until
       it answers, write the port-file, return the handle.
    """
    info = _read_port_file(db_path)
    if info and not force_start:
        port = int(info.get("port", 0))
        token = info.get("token", "")
        pid = int(info.get("pid", 0))
        if port and token and _is_alive(port, token):
            log.debug("reusing daemon pid=%d port=%d", pid, port)
            return DaemonHandle(pid, port, token, db_path)

    # Need to spawn. Pick a port + token.
    port = _free_port(DEFAULT_PORT)
    token = secrets.token_hex(16)

    # On Windows, detach via CREATE_NEW_PROCESS_GROUP | DETACHED_PROCESS.
    # On POSIX, start_new_session=True detaches from the controlling terminal.
    creationflags = 0
    start_new_session = False
    if os.name == "nt":
        creationflags = (
            getattr(subprocess, "CREATE_NEW_PROCESS_GROUP", 0)
            | getattr(subprocess, "DETACHED_PROCESS", 0)
        )
    else:
        start_new_session = True

    cmd = [
        sys.executable, "-m", "dataworm.daemon_runner",
        "--db", str(db_path),
        "--port", str(port),
        "--token", token,
    ]
    if prefer_rust:
        cmd.append("--rust")
    else:
        cmd.append("--no-rust")

    log.info("spawning daemon: %s", " ".join(cmd))
    proc = subprocess.Popen(
        cmd,
        stdin=subprocess.DEVNULL,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        creationflags=creationflags,
        start_new_session=start_new_session,
        close_fds=True,
    )
    pid = proc.pid

    # Poll until the daemon answers (it needs time to load the DB + bind).
    deadline = time.time() + _START_TIMEOUT
    while time.time() < deadline:
        if _is_alive(port, token):
            _write_port_file(db_path, pid, port, token)
            return DaemonHandle(pid, port, token, db_path)
        # If the process died immediately, don't wait the full timeout.
        if proc.poll() is not None:
            break
        time.sleep(0.2)

    # Failed to start.
    raise RuntimeError(
        f"daemon did not become ready on port {port} within {_START_TIMEOUT}s "
        f"(pid {pid} exited with code {proc.poll()})"
    )


def stop_daemon(db_path: str = DEFAULT_DB) -> dict:
    """Tell the daemon to shut down and remove the port-file."""
    info = _read_port_file(db_path)
    if not info:
        return {"ok": True, "status": "not running"}
    port = int(info.get("port", 0))
    token = info.get("token", "")
    pid = int(info.get("pid", 0))
    if port and token and _is_alive(port, token):
        handle = DaemonHandle(pid, port, token, db_path)
        try:
            handle.call("shutdown", timeout=5.0)
        except Exception:
            pass
        # Give it a moment to exit cleanly.
        time.sleep(0.5)
    _remove_port_file(db_path)
    return {"ok": True, "status": "stopped", "pid": pid, "port": port}


# ---- the HTTP server ------------------------------------------------------

class _DaemonServer(socketserver.ThreadingTCPServer):
    allow_reuse_address = True
    daemon_threads = True


class _RPCHandler(http.server.BaseHTTPRequestHandler):
    """Handles /rpc (JSON-RPC), /api/<m> (REST), /events (SSE), / (dashboard)."""

    # Set by the factory; holds the Core + bus + token + dashboard HTML + daemon.
    server_core: Core | None = None
    server_token: str = ""
    server_dashboard_html: str = ""
    server_daemon: "Daemon | None" = None

    def log_message(self, fmt, *args):
        pass  # quiet

    # ---- auth ----------------------------------------------------------
    def _check_auth(self) -> bool:
        """Deny-by-default. The bearer token may arrive via ``Authorization:
        Bearer <t>`` or a ``?token=<t>`` query param (the latter only because
        ``EventSource`` can't set headers). A missing/invalid token is refused
        — the old "no header => allow" behaviour left ``/rpc`` wide open to any
        local process. The dashboard page (``/``) is handled separately and is
        served open with the token embedded."""
        if not self.server_token:
            return False  # fail closed if somehow unconfigured
        auth = self.headers.get("Authorization", "")
        if auth.startswith("Bearer ") and auth[7:] == self.server_token:
            return True
        parsed = urllib.parse.urlparse(self.path)
        qs = urllib.parse.parse_qs(parsed.query)
        if qs.get("token", [""])[0] == self.server_token:
            return True
        return False

    # ---- GET routes ---------------------------------------------------
    def do_GET(self):
        parsed = urllib.parse.urlparse(self.path)
        path = parsed.path
        qs = urllib.parse.parse_qs(parsed.query)

        # Dashboard page: served open on loopback, but with the real token
        # injected so the browser's /api fetches and /events stream can auth.
        # Prefers the built SolidJS bundle (webapp/dist/index.html); falls
        # back to the legacy inline page when the dist is absent. Both carry
        # a __DATAWORM_TOKEN__ placeholder replaced here — one mechanism.
        if path == "/" or path.startswith("/?"):
            self._send_html(self._dashboard_html())
            return
        # Built-bundle static files (hashed Vite output). Open like "/" — the
        # browser loads them without auth headers; nothing secret in them.
        if path.startswith("/assets/"):
            self._serve_dist_asset(path[len("/assets/"):])
            return
        # Every other GET (REST + SSE) requires a valid token.
        if not self._check_auth():
            self._send_json(403, {"error": "forbidden"})
            return

        if path.startswith("/events"):
            self._handle_sse()
            return
        if path.startswith("/api/"):
            method = path[len("/api/"):].strip("/")
            # GET is reserved for read-only methods; anything that mutates or
            # triggers work must go through POST /rpc (405 Method Not Allowed,
            # not 403 — the request was authenticated, the verb is wrong).
            if method not in READ_ONLY_METHODS:
                self._send_json(405, {"error": "use POST /rpc for this method"})
                return
            # Flatten query params: single values. Drop the auth token so it
            # never leaks into the op's params.
            params = {k: (v[0] if len(v) == 1 else v)
                      for k, v in qs.items() if k != "token"}
            self._dispatch_and_respond(method, params)
            return
        self._send_json(404, {"error": f"not found: {path}"})

    # ---- POST routes --------------------------------------------------
    def do_POST(self):
        if not self._check_auth():
            self._send_json(403, {"error": "forbidden"})
            return
        parsed = urllib.parse.urlparse(self.path)
        if parsed.path != "/rpc":
            self._send_json(404, {"error": f"not found: {parsed.path}"})
            return
        length = int(self.headers.get("Content-Length", 0))
        body = self.rfile.read(length) if length else b"{}"
        try:
            req = json.loads(body.decode("utf-8"))
        except Exception as e:
            self._send_json(200, {"jsonrpc": "2.0", "error": {
                "code": -32700, "message": f"parse error: {e}"}, "id": None})
            return
        method = req.get("method", "")
        params = req.get("params", {}) or {}
        req_id = req.get("id")
        result = self.server_core.call(method, params) if self.server_core else {"error": "no core"}
        # JSON-RPC envelope.
        if "error" in result:
            resp = {"jsonrpc": "2.0", "error": {"code": -32000, "message": result["error"]}, "id": req_id}
        else:
            resp = {"jsonrpc": "2.0", "result": result, "id": req_id}
        self._send_json(200, resp)
        # If the client asked us to shut down, do so after responding.
        if method == "shutdown" and self.server_daemon is not None:
            self.server_daemon.stop()

    # ---- dispatch helper ----------------------------------------------
    def _dispatch_and_respond(self, method: str, params: dict):
        result = self.server_core.call(method, params) if self.server_core else {"error": "no core"}
        self._send_json(200, result)

    # ---- dashboard serving ---------------------------------------------
    def _dashboard_html(self) -> str:
        """The page served at ``/``: the built SolidJS bundle when
        ``webapp/dist/index.html`` exists, else the legacy inline page.
        Token injection is the SAME for both: replace the literal
        ``__DATAWORM_TOKEN__`` placeholder with this daemon's bearer token."""
        idx = DIST_DIR / "index.html"
        try:
            if idx.exists():
                return idx.read_text(encoding="utf-8").replace(
                    "__DATAWORM_TOKEN__", self.server_token)
        except Exception:
            log.exception("reading built dashboard failed; using legacy page")
        return self.server_dashboard_html.replace("__DATAWORM_TOKEN__",
                                                  self.server_token)

    def _serve_dist_asset(self, raw_name: str) -> None:
        """Serve one static file from the built dashboard dist. Deny-by-default:
        after URL-unquoting and dot-segment normalisation, the resolved path
        must still live inside DIST_DIR (blocks ``/assets/../..%2Fsecret``)."""
        import posixpath

        name = urllib.parse.unquote(raw_name).replace("\\", "/")
        rel = posixpath.normpath(name).lstrip("/")
        if not rel or rel.startswith(".."):
            self._send_json(404, {"error": "not found"})
            return
        root = DIST_DIR.resolve()
        # Browsers fetch Vite output as "/assets/<hashed file>" and those files
        # live in dist/assets/; fall back to the dist root for stray top-level
        # files (favicon.ico, …).
        target = None
        for cand in (root / "assets" / rel, root / rel):
            try:
                resolved = cand.resolve()
                resolved.relative_to(root)
            except ValueError:
                continue  # escaped the dist dir — refuse this candidate
            if resolved.is_file():
                target = resolved
                break
        if target is None:
            self._send_json(404, {"error": f"not found: {raw_name}"})
            return
        ctype = _ASSET_TYPES.get(target.suffix.lower(), "application/octet-stream")
        try:
            body = target.read_bytes()
        except OSError:
            self._send_json(404, {"error": f"unreadable: {raw_name}"})
            return
        self.send_response(200)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Cache-Control", "public, max-age=3600")
        self.end_headers()
        self.wfile.write(body)

    # ---- SSE ----------------------------------------------------------
    def _handle_sse(self):
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.send_header("Cache-Control", "no-cache")
        self.send_header("Connection", "keep-alive")
        self.end_headers()
        import queue
        # Bounded queue: a slow browser must not let the daemon OOM by piling
        # up millions of events. On full we drop the event and bump a dropped
        # counter (surfaced to the client as a `dropped` event) — backpressure
        # that keeps the daemon alive at the cost of a sampled view.
        q: queue.Queue = queue.Queue(maxsize=10000)
        dropped = {"n": 0}

        def _put(ev):
            try:
                q.put_nowait(ev)
            except queue.Full:
                dropped["n"] += 1
                # Periodically surface the drop count so the dashboard can show
                # "sampled" instead of silently losing events.
                if dropped["n"] % 1000 == 0:
                    try:
                        q.put_nowait({"kind": "dropped", "count": dropped["n"]})
                    except queue.Full:
                        pass

        bus = self.server_core.bus if self.server_core else None
        # Subscribe BEFORE replaying history: an event emitted between the
        # replay snapshot and a later subscribe() would be lost forever.
        # Overlap is harmless — the seq filter below drops duplicates.
        if bus is not None:
            bus.subscribe(_put)
        last_seq = 0
        try:
            # Replay buffered events from the daemon, then stream live.
            daemon = self.server_daemon
            if daemon is not None:
                for ev in daemon.replay_events(0):
                    if ev.get("seq", 0) > last_seq:
                        line = f"data: {json.dumps(ev, default=str)}\n\n"
                        try:
                            self.wfile.write(line.encode("utf-8"))
                            self.wfile.flush()
                        except (BrokenPipeError, ConnectionResetError):
                            return
                        last_seq = ev.get("seq", 0)
            while True:
                try:
                    ev = q.get(timeout=1.0)
                except queue.Empty:
                    self.wfile.write(b": ping\n\n")
                    self.wfile.flush()
                    continue
                if ev.get("seq", 0) > last_seq:
                    line = f"data: {json.dumps(ev, default=str)}\n\n"
                    self.wfile.write(line.encode("utf-8"))
                    self.wfile.flush()
                    last_seq = ev.get("seq", 0)
        except (BrokenPipeError, ConnectionResetError):
            pass
        finally:
            if bus is not None:
                try:
                    bus.unsubscribe(_put)
                except Exception:
                    pass

    # ---- response helpers ---------------------------------------------
    def _send_json(self, code: int, obj: Any):
        body = json.dumps(obj, default=str).encode("utf-8")
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def _send_html(self, html: str):
        body = html.encode("utf-8")
        self.send_response(200)
        self.send_header("Content-Type", "text/html; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)


def _make_handler(core: Core, token: str, dashboard_html: str, daemon: "Daemon | None" = None):
    """Create a handler class bound to a specific Core + token."""
    class _Bound(_RPCHandler):
        server_core = core
        server_token = token
        server_dashboard_html = dashboard_html
        server_daemon = daemon
    return _Bound


# ---- the daemon itself ----------------------------------------------------

class Daemon:
    """The centralized server process. Owns a Core + HTTP server."""

    def __init__(self, db_path: str = DEFAULT_DB, port: int = DEFAULT_PORT,
                 token: str = "", prefer_rust: bool = True) -> None:
        self.db_path = db_path
        self.port = port
        self.token = token or secrets.token_hex(16)
        self.prefer_rust = prefer_rust
        self.core = Core(db_path=db_path, prefer_rust=prefer_rust)
        # Bounded ring buffer of recent events for SSE replay. The daemon is
        # long-lived (it watches dirs), so an unbounded list would grow forever;
        # cap it and drop the oldest when full. New clients still catch up on
        # the recent window; the live stream carries everything from connect on.
        self.event_log: list[dict] = []
        self._event_log_cap = 5000
        self._log_lock = threading.Lock()
        self.core.bus.subscribe(self._record_event)
        self._httpd: _DaemonServer | None = None
        self._stop = threading.Event()

    def _record_event(self, ev: dict) -> None:
        with self._log_lock:
            self.event_log.append(ev)
            if len(self.event_log) > self._event_log_cap:
                # Drop in chunks to amortize the cost on a hot emit path.
                del self.event_log[: len(self.event_log) - self._event_log_cap]

    def replay_events(self, since_seq: int = 0) -> list[dict]:
        with self._log_lock:
            return [e for e in self.event_log if e.get("seq", 0) > since_seq]

    def start(self) -> None:
        """Bind the HTTP server and write the port-file. Does not block."""
        from dataworm.live import HTML_PAGE
        handler = _make_handler(self.core, self.token, HTML_PAGE, daemon=self)
        self._httpd = _DaemonServer(("127.0.0.1", self.port), handler)
        self.port = self._httpd.server_address[1]  # actual bound port
        _write_port_file(self.db_path, os.getpid(), self.port, self.token)
        log.info("daemon listening on 127.0.0.1:%d (db=%s)", self.port, self.db_path)

    def serve_forever(self) -> None:
        """Block until stopped. Call ``start()`` first.

        Uses ``serve_forever()`` on the threading server so that ``shutdown()``
        can unblock it from another thread (e.g. the shutdown RPC handler).
        """
        if self._httpd is None:
            self.start()
        try:
            self._httpd.serve_forever(poll_interval=0.2)
        finally:
            self._httpd.server_close()
            _remove_port_file(self.db_path)

    def stop(self) -> None:
        """Signal the server loop to exit (called from the shutdown RPC)."""
        self._stop.set()
        # Stop every filesystem watcher so background threads don't outlive us.
        try:
            self.core.stop_watchers()
        except Exception:
            log.exception("stopping watchers on shutdown failed")
        if self._httpd is not None:
            # shutdown() must be called from a different thread than serve_forever.
            threading.Thread(target=self._httpd.shutdown, daemon=True).start()
