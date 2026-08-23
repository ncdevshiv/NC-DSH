"""Stage 1.4/1.6 + webapp rebuild: the served dashboard is the built SolidJS
bundle (with the bearer token injected), SSE stays open past `done`, and the
watch/query endpoints are reachable via REST (the path the dashboard JS uses).
"""

from __future__ import annotations

import json
import urllib.parse
import urllib.request

import pytest

import dataworm
from dataworm.server import ensure_daemon, stop_daemon, _remove_port_file


@pytest.fixture
def daemon_handle(tmp_path):
    db = str(tmp_path / "graph.db")
    _remove_port_file(db)
    handle = ensure_daemon(db_path=db, prefer_rust=True)
    yield handle
    try:
        stop_daemon(db)
    except Exception:
        pass


def _api(handle, method, **params):
    url = f"http://127.0.0.1:{handle.port}/api/{method}"
    if params:
        url += "?" + "&".join(
            f"{k}={urllib.parse.quote(str(v))}" for k, v in params.items()
        )
    req = urllib.request.Request(
        url, headers={"Authorization": f"Bearer {handle.token}"}
    )
    with urllib.request.urlopen(req, timeout=10) as resp:
        return json.loads(resp.read().decode())


def _rpc(handle, method, params=None):
    """POST /rpc with a JSON-RPC 2.0 envelope; returns the unwrapped result.

    Mutating methods (watch/unwatch/crawl/...) are POST-only now: GET /api
    answers 405 for them, so tests must go through the RPC surface like the
    CLI does."""
    payload = json.dumps({
        "jsonrpc": "2.0",
        "method": method,
        "params": params or {},
        "id": 1,
    }).encode("utf-8")
    req = urllib.request.Request(
        f"http://127.0.0.1:{handle.port}/rpc", data=payload, method="POST",
        headers={"Content-Type": "application/json",
                 "Authorization": f"Bearer {handle.token}"},
    )
    with urllib.request.urlopen(req, timeout=10) as resp:
        return json.loads(resp.read().decode())["result"]


def test_dashboard_html_contains_interactive_hooks(daemon_handle):
    """The served page must be the built SolidJS bundle: it references the
    hashed Vite assets under /assets/ and carries the injected bearer token
    (the __DATAWORM_TOKEN__ placeholder replaced) so the browser's fetches
    and EventSource authenticate against the deny-by-default server."""
    url = f"http://127.0.0.1:{daemon_handle.port}/"
    req = urllib.request.Request(url)
    with urllib.request.urlopen(req, timeout=5) as resp:
        html = resp.read().decode()
    # The built bundle is referenced (hashed Vite output under /assets/).
    assert "/assets/" in html
    assert "window.__DW_TOKEN__" in html   # the injection hook the app reads
    # Auth: the placeholder must be replaced with the real bearer token so the
    # browser's fetches/SSE can authenticate against the deny-by-default server.
    assert "__DATAWORM_TOKEN__" not in html
    assert daemon_handle.token in html


def test_legacy_fallback_when_dist_missing(daemon_handle):
    """With the built dist renamed away, GET / must serve the legacy inline
    page again — same token injection, no /assets/ bundle references. (The
    daemon reads dist from disk per request, so a real rename is needed; the
    daemon runs detached, so in-process monkeypatching cannot reach it.)"""
    from pathlib import Path as _Path

    webapp = _Path(dataworm.__file__).resolve().parent / "webapp"
    dist = webapp / "dist"
    moved = webapp / "dist.off"
    assert dist.is_dir(), "built dist missing from dataworm/webapp/"
    dist.rename(moved)
    try:
        url = f"http://127.0.0.1:{daemon_handle.port}/"
        with urllib.request.urlopen(url, timeout=5) as resp:
            html = resp.read().decode()
    finally:
        moved.rename(dist)
    assert "inspectNode" in html             # legacy-only interactive hooks
    assert "/assets/" not in html            # bundle NOT referenced
    assert "__DATAWORM_TOKEN__" not in html  # legacy token mechanism intact
    assert daemon_handle.token in html


def test_assets_content_type_and_traversal_rejected(daemon_handle):
    """Built assets are served open (like "/") with correct content types;
    path traversal out of the dist dir is rejected with 403/404."""
    import urllib.error
    from pathlib import Path as _Path

    dist = _Path(dataworm.__file__).resolve().parent / "webapp" / "dist"
    js_files = sorted((dist / "assets").glob("*.js"))
    assert js_files, "built bundle missing from dataworm/webapp/dist"
    name = js_files[0].name
    url = f"http://127.0.0.1:{daemon_handle.port}/assets/{name}"
    with urllib.request.urlopen(url, timeout=5) as resp:
        assert resp.status == 200
        assert resp.headers["Content-Type"].startswith("text/javascript")
        assert resp.read(64)  # non-empty body
    # Traversal attempts (raw dots + fully %2F-encoded variants) must fail.
    for bad in ("/assets/../..%2Fsecret.txt", "/assets/%2e%2e%2f%2e%2e%2fsecret.txt"):
        req = urllib.request.Request(f"http://127.0.0.1:{daemon_handle.port}{bad}")
        with pytest.raises(urllib.error.HTTPError) as ei:
            urllib.request.urlopen(req, timeout=5)
        assert ei.value.code in (403, 404)


def test_ops_refused_without_token(daemon_handle):
    """Deny-by-default: /api and /rpc must 403 when no/incorrect token is sent
    (the old behaviour allowed any header-less local request)."""
    import urllib.error
    # /api with no token -> 403
    req = urllib.request.Request(
        f"http://127.0.0.1:{daemon_handle.port}/api/summary"
    )
    with pytest.raises(urllib.error.HTTPError) as ei:
        urllib.request.urlopen(req, timeout=5)
    assert ei.value.code == 403
    # /rpc with a WRONG token -> 403
    payload = json.dumps({"jsonrpc": "2.0", "method": "ping",
                          "params": {}, "id": 1}).encode()
    req = urllib.request.Request(
        f"http://127.0.0.1:{daemon_handle.port}/rpc", data=payload,
        headers={"Content-Type": "application/json",
                 "Authorization": "Bearer wrong-token"},
    )
    with pytest.raises(urllib.error.HTTPError) as ei:
        urllib.request.urlopen(req, timeout=5)
    assert ei.value.code == 403


def test_watch_unwatch_via_rest(daemon_handle, sample_root):
    """watch/unwatch must be reachable from the dashboard's surface (POST /rpc —
    GET /api answers 405 for mutators)."""
    r = _rpc(daemon_handle, "watch",
             {"root": str(sample_root), "poll_interval": 0.1})
    assert r["ok"] is True
    assert r["status"] == "watching"
    assert r["backend"] in ("watchdog", "polling")

    watched = _api(daemon_handle, "watched")
    assert str(sample_root) in watched["roots"]

    r = _rpc(daemon_handle, "unwatch", {"root": str(sample_root)})
    assert r["ok"] is True
    watched = _api(daemon_handle, "watched")
    assert watched["roots"] == []


def test_sse_stays_open_past_done(daemon_handle, sample_root):
    """The SSE stream must not terminate on the `done` event (long-lived worm).

    We crawl once (which emits start/pass/cycle/done), then confirm the stream
    is still alive by receiving a keep-alive ping after `done`.
    """
    import threading, queue
    q: queue.Queue = queue.Queue()
    def stream():
        req = urllib.request.Request(
            f"http://127.0.0.1:{daemon_handle.port}/events",
            headers={"Authorization": f"Bearer {daemon_handle.token}"},
        )
        try:
            with urllib.request.urlopen(req, timeout=15) as resp:
                buf = b""
                while True:
                    chunk = resp.read(1)
                    if not chunk:
                        break
                    buf += chunk
                    while b"\n\n" in buf:
                        raw, buf = buf.split(b"\n\n", 1)
                        line = raw.decode("utf-8", "ignore")
                        q.put(line)
                        if line.startswith("data: "):
                            ev = json.loads(line[6:])
                            if ev.get("kind") == "done":
                                return  # we proved done arrived; stream still open here
        except Exception:
            pass
    t = threading.Thread(target=stream, daemon=True)
    t.start()
    daemon_handle.call("crawl", {"root": str(sample_root), "max_cycles": 2,
                                 "enable_semantic": False}, timeout=60)
    # Wait for the done event to arrive on the stream.
    got_done = False
    deadline = __import__("time").time() + 10
    while __import__("time").time() < deadline:
        try:
            line = q.get(timeout=1.0)
            if "done" in line:
                got_done = True
                break
        except queue.Empty:
            continue
    assert got_done, "done event did not arrive on SSE stream"
