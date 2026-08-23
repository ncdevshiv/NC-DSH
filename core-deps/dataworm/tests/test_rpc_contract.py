"""JSON-RPC contract tests: the /rpc endpoint must honour JSON-RPC 2.0.

These verify the wire protocol the daemon exposes — the contract that makes
the runner "globally callable" from any JSON-RPC client.
"""

from __future__ import annotations

import json
import urllib.request

import pytest

from dataworm.server import ensure_daemon, stop_daemon, _remove_port_file, DaemonHandle


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


def _rpc(handle: DaemonHandle, method: str, params: dict | None = None, id: int = 1) -> dict:
    payload = json.dumps({
        "jsonrpc": "2.0",
        "method": method,
        "params": params or {},
        "id": id,
    }).encode("utf-8")
    url = f"http://127.0.0.1:{handle.port}/rpc"
    req = urllib.request.Request(url, data=payload, method="POST",
                                 headers={
                                     "Content-Type": "application/json",
                                     "Authorization": f"Bearer {handle.token}",
                                 })
    with urllib.request.urlopen(req, timeout=30) as resp:
        return json.loads(resp.read().decode("utf-8"))


def test_jsonrpc_success_envelope(daemon_handle):
    resp = _rpc(daemon_handle, "ping")
    assert resp["jsonrpc"] == "2.0"
    assert resp["id"] == 1
    assert "result" in resp
    assert resp["result"]["ok"] is True


def test_jsonrpc_error_envelope(daemon_handle):
    resp = _rpc(daemon_handle, "nonexistent_method")
    assert resp["jsonrpc"] == "2.0"
    assert "error" in resp
    assert resp["error"]["code"] == -32000
    assert "unknown method" in resp["error"]["message"]


def test_rest_endpoint(daemon_handle):
    """GET /api/<method> must return the same result as JSON-RPC."""
    url = f"http://127.0.0.1:{daemon_handle.port}/api/ping"
    req = urllib.request.Request(url, headers={
        "Authorization": f"Bearer {daemon_handle.token}"})
    with urllib.request.urlopen(req, timeout=5) as resp:
        data = json.loads(resp.read().decode("utf-8"))
    assert data["ok"] is True


def test_crawl_via_rpc(daemon_handle, sample_root):
    """A full crawl via JSON-RPC must produce a valid graph."""
    resp = _rpc(daemon_handle, "crawl", {
        "root": str(sample_root),
        "max_cycles": 5,
    }, id=42)
    assert resp["jsonrpc"] == "2.0"
    assert resp["id"] == 42
    result = resp["result"]
    assert result["converged"] is True
    assert result["nodes"] > 0
    assert result["edges_references"] > 0


def test_impact_via_rpc(daemon_handle, sample_root):
    """Impact query via JSON-RPC must return the blast radius."""
    _rpc(daemon_handle, "crawl", {"root": str(sample_root), "max_cycles": 5})
    resp = _rpc(daemon_handle, "impact", {"path": "c.py"})
    result = resp["result"]
    assert result["target"] == "c.py"
    # Federated impact returns rich entries: [{id: "b.py"}, ...].
    direct_ids = [d["id"] if isinstance(d, dict) else d for d in result["direct"]]
    transitive_ids = [d["id"] if isinstance(d, dict) else d for d in result["transitive"]]
    assert "b.py" in direct_ids
    assert "a.py" in transitive_ids


def test_dashboard_html_served(daemon_handle):
    """GET / must return the dashboard HTML page."""
    url = f"http://127.0.0.1:{daemon_handle.port}/"
    req = urllib.request.Request(url, headers={
        "Authorization": f"Bearer {daemon_handle.token}"})
    with urllib.request.urlopen(req, timeout=5) as resp:
        html = resp.read().decode("utf-8")
    assert "<html" in html.lower()
    assert "DataWorm" in html or "dataworm" in html
