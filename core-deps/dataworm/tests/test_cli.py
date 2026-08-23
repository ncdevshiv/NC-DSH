"""CLI tests: the thin client (dataworm.cli) without real daemons or sockets.

Covers four areas:
  1. In-process end-to-end (--no-daemon): crawl -> summary -> impact -> context
     -> neighbors -> search over a tmp_path tree, asserting JSON payload content
     and that every command persists/reloads through the SQLite DB (each CLI
     invocation builds a fresh Core).
  2. Error paths: unknown impact/context/neighbor targets and crawling a
     nonexistent root surface clean ``SystemExit``s (never tracebacks). Note:
     ``sys.exit("message")`` carries the message as ``SystemExit.code`` in a
     real process it exits with status 1.
  3. Client plumbing, fully monkeypatched: port-file round-trip, ``_rpc_call``
     URL/auth/body construction, ``ensure_daemon`` reuse-vs-respawn, and the
     friendly crawl-timeout handling.
  4. Flag wiring: what ``--no-daemon`` / ``--no-rust`` actually do versus what
     the module docstring promises.
"""

from __future__ import annotations

import contextlib
import io
import json
import subprocess
import sys
from pathlib import Path
from unittest.mock import MagicMock

import pytest

import dataworm.cli as cli
import dataworm.server as server
from dataworm.core import Core, _try_import_rust
from dataworm.server import (
    DaemonHandle,
    _daemon_path,
    _read_port_file,
    _remove_port_file,
    _write_port_file,
)


# ---- helpers ---------------------------------------------------------------

_SHARED_PARA = (
    "quantum flux capacitor resonance harmonic oscillator lattice "
    "eigenvalue manifold tensor gradient descent optimization convergence "
    "trajectory manifold embedding vector semantic discovery linkage graph "
) * 4


def build_cli_tree(root: Path) -> Path:
    """Tiny project tree: a.py imports b.py, c.md links a.py, plus exact dups."""
    root.mkdir(parents=True, exist_ok=True)
    (root / "a.py").write_text("import b\n\nprint('a')\n", encoding="utf-8")
    (root / "b.py").write_text(
        "def helper():\n    return 42\n\n\nprint('b')\n", encoding="utf-8"
    )
    (root / "c.md").write_text("# C\n\nSee [entry](a.py).\n", encoding="utf-8")
    (root / "dup1.txt").write_text(_SHARED_PARA, encoding="utf-8")
    (root / "dup2.txt").write_text(_SHARED_PARA, encoding="utf-8")
    return root


def db_of(root: Path) -> str:
    return str(Path(root) / ".dataworm" / "graph.db")


def _capture(fn, *args, **kwargs) -> str:
    buf = io.StringIO()
    with contextlib.redirect_stdout(buf):
        fn(*args, **kwargs)
    return buf.getvalue()


def run_cli(*argv) -> str:
    """Invoke cli.main(argv), capturing stdout. Raises SystemExit on CLI errors."""
    return _capture(cli.main, list(argv))


def run_cli_json(*argv):
    """Invoke a CLI command with --json and return the parsed payload."""
    assert "--json" in argv, "run_cli_json requires --json in argv"
    return json.loads(run_cli(*argv))


def forbid_daemon(monkeypatch) -> None:
    """Guard: any attempt to touch the daemon plumbing fails the test loudly."""
    def _boom(*args, **kwargs):
        raise AssertionError("ensure_daemon must not run in --no-daemon mode")

    monkeypatch.setattr(server, "ensure_daemon", _boom)


# ---- 1. in-process end-to-end (--no-daemon) --------------------------------

def test_no_daemon_full_command_chain(tmp_path, monkeypatch):
    """crawl -> summary -> impact -> context -> neighbors -> search, in-process.

    Every invocation builds a fresh Core over the persisted SQLite DB, so this
    doubles as a persistence round-trip test of the CLI contract.
    """
    forbid_daemon(monkeypatch)
    root = build_cli_tree(tmp_path / "proj")
    db = db_of(root)

    # crawl (text output; crawl has no --json stdout flag)
    out = run_cli("crawl", str(root), "--no-daemon")
    assert "converged=True" in out
    assert "nodes=" in out and "edges=" in out
    assert Path(db).exists()

    # summary
    s = run_cli_json("summary", "--db", db, "--no-daemon", "--json")
    assert s["nodes"] > 0 and s["edges"] > 0
    # a.py -> b.py and c.md -> a.py
    assert s["edges_references"] >= 2
    # dup1.txt / dup2.txt are byte-identical
    assert s["edges_duplicate_of"] >= 1
    assert s["node_kinds"]["file"] >= 5

    # impact: blast radius of b.py is a.py (direct), c.md (transitive)
    imp = run_cli_json("impact", "b.py", "--db", db, "--no-daemon", "--json")
    assert imp["target"] == "b.py"
    assert "a.py" in [d["id"] for d in imp["direct"]]
    assert "c.md" in [t["id"] for t in imp["transitive"]]
    assert imp["total_affected"] >= 2

    imp_a = run_cli_json("impact", "a.py", "--db", db, "--no-daemon", "--json")
    assert "c.md" in [d["id"] for d in imp_a["direct"]]

    # context: node bundle + reference links in both directions
    ctx = run_cli_json("context", "a.py", "--db", db, "--no-daemon", "--json")
    assert ctx["node"]["id"] == "a.py"
    ref_ids = {lnk["id"] for lnk in ctx["links"] if lnk["type"] == "references"}
    assert ref_ids == {"b.py", "c.md"}
    assert ctx["link_counts"].get("references") == 2
    assert ctx["impact"]["target"] == "a.py"

    # neighbors: untyped within 2 hops contains the dependents
    nb = run_cli_json(
        "neighbors", "a.py", "--db", db, "--no-daemon", "--json", "--depth", "2"
    )
    assert nb["target"] == "a.py"
    assert {"b.py", "c.md"} <= {n["id"] for n in nb["neighbors"]}

    # neighbors: filtered to references at depth 1 -> exactly the dependents
    nbr = run_cli_json(
        "neighbors", "a.py", "--db", db, "--no-daemon", "--json",
        "--type", "references", "--depth", "1",
    )
    assert {n["id"] for n in nbr["neighbors"]} == {"b.py", "c.md"}

    # search: substring over ids/paths, deterministic id sort
    hits = run_cli_json("search", ".py", "--db", db, "--no-daemon", "--json")
    hit_ids = {h["id"] for h in hits["results"]}
    assert {"a.py", "b.py"} <= hit_ids
    for h in hits["results"]:
        assert h["kind"] == "file"
        assert Path(h["path"]).name == h["id"]

    exact = run_cli_json("search", "b.py", "--db", db, "--no-daemon", "--json")
    assert [h["id"] for h in exact["results"]] == ["b.py"]


def test_summary_root_round_trips_after_reload(tmp_path):
    root = build_cli_tree(tmp_path / "proj")
    db = db_of(root)
    run_cli("crawl", str(root), "--no-daemon")
    s = run_cli_json("summary", "--db", db, "--no-daemon", "--json")
    assert Path(s["root"]).resolve() == root.resolve()


def test_json_flag_switches_machine_readable_output(tmp_path, monkeypatch):
    forbid_daemon(monkeypatch)
    root = build_cli_tree(tmp_path / "proj")
    db = db_of(root)
    run_cli("crawl", str(root), "--no-daemon")

    raw = run_cli("summary", "--db", db, "--no-daemon", "--json").strip()
    payload = json.loads(raw)  # must be pure parseable JSON
    assert payload["nodes"] > 0

    pretty = run_cli("summary", "--db", db, "--no-daemon")
    assert not pretty.lstrip().startswith(("{", "["))  # human format, not JSON
    assert "nodes" in pretty


def test_init_no_daemon_crawls_once_without_background_watcher(tmp_path, monkeypatch):
    forbid_daemon(monkeypatch)
    root = build_cli_tree(tmp_path / "proj")
    out = run_cli("init", str(root), "--no-daemon")
    assert "init: --no-daemon mode" in out
    assert "converged=True" in out
    assert "graph saved to" in out
    assert Path(db_of(root)).exists()


def test_no_rust_forces_python_backend_but_graph_still_builds(tmp_path):
    root = build_cli_tree(tmp_path / "proj")
    db = db_of(root)
    run_cli("crawl", str(root), "--no-daemon", "--no-rust")

    # --no-rust means prefer_rust=False -> Core.rust is None -> backend "python".
    assert cli._run_inprocess("ping", {}, db, prefer_rust=False)["backend"] == "python"
    assert Core(db_path=db, prefer_rust=False).rust is None

    # The Python fallback still produces the full reference graph.
    s = run_cli_json("summary", "--db", db, "--no-daemon", "--json", "--no-rust")
    assert s["edges_references"] >= 2

    # Without --no-rust the CLI prefers Rust iff the extension is importable.
    expected = "rust" if _try_import_rust() is not None else "python"
    assert cli._run_inprocess("ping", {}, db, prefer_rust=True)["backend"] == expected


# ---- 2. error paths ---------------------------------------------------------

def test_impact_unknown_path_exits_with_error(tmp_path, monkeypatch):
    forbid_daemon(monkeypatch)
    root = build_cli_tree(tmp_path / "proj")
    db = db_of(root)
    run_cli("crawl", str(root), "--no-daemon")

    with pytest.raises(SystemExit) as excinfo:
        run_cli("impact", "does/not/exist.py", "--db", db, "--no-daemon")
    # sys.exit("<message>") -> nonzero exit in a real process; the string rides
    # on SystemExit.code in-process.
    assert isinstance(excinfo.value.code, str)
    assert "unknown path" in excinfo.value.code


def test_context_unknown_path_exits_even_with_json(tmp_path, monkeypatch):
    forbid_daemon(monkeypatch)
    root = build_cli_tree(tmp_path / "proj")
    db = db_of(root)
    run_cli("crawl", str(root), "--no-daemon")

    with pytest.raises(SystemExit) as excinfo:
        run_cli("context", "ghost.py", "--db", db, "--no-daemon", "--json")
    assert "unknown path" in str(excinfo.value.code)


def test_neighbors_unknown_path_exits_with_error(tmp_path, monkeypatch):
    forbid_daemon(monkeypatch)
    root = build_cli_tree(tmp_path / "proj")
    db = db_of(root)
    run_cli("crawl", str(root), "--no-daemon")

    with pytest.raises(SystemExit) as excinfo:
        run_cli("neighbors", "ghost.py", "--db", db, "--no-daemon", "--json")
    assert "unknown path" in str(excinfo.value.code)


def test_crawl_missing_root_fails_before_daemon_work(tmp_path, monkeypatch):
    forbid_daemon(monkeypatch)
    missing = str(tmp_path / "nope")
    # Clean failure both with and without --no-daemon (the is_dir guard fires
    # before any daemon plumbing would be touched).
    for extra in ([], ["--no-daemon"]):
        with pytest.raises(SystemExit) as excinfo:
            run_cli("crawl", missing, *extra)
        assert isinstance(excinfo.value.code, str)
        assert "is not a directory" in excinfo.value.code


# ---- 3. client plumbing (monkeypatched, no sockets) ------------------------

class _FakeResponse:
    """Minimal urlopen() response: context manager + read()."""

    def __init__(self, payload: bytes):
        self._payload = payload

    def read(self) -> bytes:
        return self._payload

    def __enter__(self):
        return self

    def __exit__(self, *exc_info):
        return False


def test_port_file_round_trip(tmp_path):
    db = str(tmp_path / "graph.db")
    assert _read_port_file(db) is None

    _write_port_file(db, pid=4242, port=9999, token="tok123")
    info = _read_port_file(db)
    assert info is not None
    assert info["pid"] == 4242
    assert info["port"] == 9999
    assert info["token"] == "tok123"
    assert info["db"] == db
    assert _daemon_path(db).exists()  # daemon.json lives next to the graph db

    _remove_port_file(db)
    assert _read_port_file(db) is None
    _remove_port_file(db)  # removing again is idempotent


def test_port_file_read_tolerates_garbage(tmp_path):
    db = str(tmp_path / "graph.db")
    path = _daemon_path(db)  # creates .dataworm/ for us
    path.write_text("this is not json{", encoding="utf-8")
    assert _read_port_file(db) is None


def test_rpc_call_builds_url_auth_and_jsonrpc_body(monkeypatch):
    captured = {}

    def fake_urlopen(req, timeout=None):
        captured["url"] = req.full_url
        captured["method"] = req.get_method()
        captured["headers"] = {k.lower(): v for k, v in req.headers.items()}
        captured["body"] = req.data
        captured["timeout"] = timeout
        return _FakeResponse(json.dumps(
            {"jsonrpc": "2.0", "id": 1, "result": {"ok": True}}).encode("utf-8"))

    monkeypatch.setattr("urllib.request.urlopen", fake_urlopen)
    handle = DaemonHandle(pid=1, port=8642, token="sekret", db_path="x.db")

    result = server._rpc_call(handle, "impact", {"path": "a.py"}, timeout=7.5)

    assert result == {"ok": True}
    assert captured["url"] == "http://127.0.0.1:8642/rpc"
    assert captured["method"] == "POST"
    assert captured["headers"]["authorization"] == "Bearer sekret"
    assert captured["headers"]["content-type"] == "application/json"
    assert captured["timeout"] == 7.5
    body = json.loads(captured["body"].decode("utf-8"))
    assert body == {"jsonrpc": "2.0", "method": "impact",
                    "params": {"path": "a.py"}, "id": 1}


def test_rpc_call_surfaces_error_envelopes(monkeypatch):
    responses = iter([
        {"jsonrpc": "2.0", "id": 1, "error": {"code": -32000, "message": "boom"}},
        {"jsonrpc": "2.0", "id": 1, "error": "plain string"},
        {"jsonrpc": "2.0", "id": 1, "result": {}},
    ])

    def fake_urlopen(req, timeout=None):
        return _FakeResponse(json.dumps(next(responses)).encode("utf-8"))

    monkeypatch.setattr("urllib.request.urlopen", fake_urlopen)
    handle = DaemonHandle(pid=1, port=1, token="t", db_path="d")

    assert server._rpc_call(handle, "m") == {"code": -32000, "message": "boom"}
    assert server._rpc_call(handle, "m") == {"error": "plain string"}
    assert server._rpc_call(handle, "m") == {}


def test_ensure_daemon_reuses_live_daemon_without_spawning(monkeypatch, tmp_path):
    db = str(tmp_path / "graph.db")
    _write_port_file(db, pid=111, port=5555, token="livetoken")

    seen = []
    monkeypatch.setattr(
        server, "_is_alive", lambda port, token: seen.append((port, token)) or True
    )
    popen = MagicMock(side_effect=AssertionError("ensure_daemon must not spawn"))
    monkeypatch.setattr(subprocess, "Popen", popen)

    handle = server.ensure_daemon(db_path=db, prefer_rust=True)

    assert (handle.pid, handle.port, handle.token, handle.db_path) == \
        (111, 5555, "livetoken", db)
    assert seen == [(5555, "livetoken")]
    popen.assert_not_called()


def test_ensure_daemon_respawns_dead_daemon(monkeypatch, tmp_path):
    db = str(tmp_path / "graph.db")
    _write_port_file(db, pid=222, port=6666, token="oldtoken")

    probes = {"n": 0}

    def fake_alive(port, token):
        probes["n"] += 1
        return probes["n"] > 1  # first probe: stale daemon; post-spawn: alive

    monkeypatch.setattr(server, "_is_alive", fake_alive)
    monkeypatch.setattr(server, "_free_port", lambda preferred=server.DEFAULT_PORT: 7777)

    class _FakeProc:
        pid = 33333

        def poll(self):
            return None

    spawned = {}

    def fake_popen(cmd, **kwargs):
        spawned["cmd"] = cmd
        return _FakeProc()

    monkeypatch.setattr(subprocess, "Popen", fake_popen)

    handle = server.ensure_daemon(db_path=db, prefer_rust=False)

    assert handle.port == 7777
    assert handle.pid == 33333
    assert handle.token and handle.token != "oldtoken"
    assert handle.db_path == db
    cmd = spawned["cmd"]
    assert cmd[0] == sys.executable
    assert cmd[1:3] == ["-m", "dataworm.daemon_runner"]
    assert "--port" in cmd and "7777" in cmd
    assert "--no-rust" in cmd and "--rust" not in cmd  # prefer_rust=False wiring
    # The port file now records the *new* daemon.
    info = _read_port_file(db)
    assert info["pid"] == 33333
    assert info["port"] == 7777
    assert info["token"] == handle.token
    assert probes["n"] >= 2  # stale probe + at least one readiness poll


class _ExplodingHandle:
    """DaemonHandle stand-in whose call() always raises."""

    port = 1234

    def __init__(self, exc):
        self._exc = exc

    def call(self, method, params=None, timeout=None):
        raise self._exc


def test_crawl_via_daemon_friendly_timeout_prints_guidance_and_exits_124():
    """A timed-out crawl RPC prints guidance and exits 124 (the conventional
    "timed out" code), not a traceback and not a fake success."""
    handle = _ExplodingHandle(Exception("socket.timeout: The read operation timed out"))
    buf = io.StringIO()
    with contextlib.redirect_stdout(buf):
        with pytest.raises(SystemExit) as excinfo:
            cli._crawl_via_daemon(handle, {"root": "x"})
    assert excinfo.value.code == 124  # the RPC gave up; the daemon keeps crawling
    assert "still running in the background" in buf.getvalue()
    assert "dataworm status" in buf.getvalue()


def test_crawl_via_daemon_reraises_non_timeout_errors():
    handle = _ExplodingHandle(ValueError("connection refused"))
    with pytest.raises(ValueError, match="connection refused"):
        cli._crawl_via_daemon(handle, {})


def test_status_without_port_file_reports_not_running(tmp_path):
    out = run_cli("status", "--out", str(tmp_path / "graph.db"))
    assert "daemon: not running" in out


def test_status_reports_stale_daemon_when_ping_fails(tmp_path, monkeypatch):
    db = str(tmp_path / "graph.db")
    _write_port_file(db, pid=99, port=7000, token="t")
    monkeypatch.setattr(server, "_is_alive", lambda port, token: False)
    out = run_cli("status", "--out", db)
    assert "stale" in out
    assert "pid=99" in out and "port=7000" in out


def test_stop_without_daemon_is_clean(tmp_path):
    out = run_cli("stop", "--out", str(tmp_path / "graph.db"))
    assert "not running" in out


# ---- 4. flag / parser wiring ------------------------------------------------

def test_parser_flags_wire_to_command_functions():
    p = cli.build_parser()

    a = p.parse_args(
        ["impact", "a.py", "--db", "d.db", "--json", "--no-daemon", "--no-rust"]
    )
    assert a.func is cli.cmd_impact
    assert (a.path, a.db, a.json, a.no_daemon, a.no_rust) == \
        ("a.py", "d.db", True, True, True)

    a = p.parse_args(["crawl", "some/dir"])
    assert a.func is cli.cmd_crawl
    assert a.max_cycles == 5
    assert a.no_daemon is False and a.no_rust is False
    a = p.parse_args(["crawl", "some/dir", "--no-daemon", "--no-rust"])
    assert a.no_daemon is True and a.no_rust is True

    a = p.parse_args([
        "neighbors", "a.py", "--type", "references", "--type", "contains",
        "--depth", "2",
    ])
    assert a.func is cli.cmd_neighbors
    assert a.type == ["references", "contains"]
    assert a.depth == 2

    a = p.parse_args(["search", "needle", "--limit", "3"])
    assert a.func is cli.cmd_search
    assert a.limit == 3

    a = p.parse_args(["summary"])
    assert a.func is cli.cmd_summary
    assert a.json is False

    a = p.parse_args(["init", "somewhere"])
    assert a.func is cli.cmd_init
    assert a.web is True  # dashboard auto-open is the documented default
    a = p.parse_args(["init", "somewhere", "--no-watch", "--no-web"])
    assert a.no_watch is True and a.web is False

    assert p.parse_args(["status"]).func is cli.cmd_status
    assert p.parse_args(["stop"]).func is cli.cmd_stop


def test_db_path_resolution(tmp_path):
    target = tmp_path / "proj"
    target.mkdir()
    # Default: federated per-dir storage inside the crawled directory.
    assert cli._db_path_for(str(target), None) == \
        str(target.resolve() / ".dataworm" / "graph.db")
    # Explicit --out wins unchanged.
    assert cli._db_path_for(str(target), "custom.db") == "custom.db"
