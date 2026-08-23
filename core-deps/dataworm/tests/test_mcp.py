"""MCP stdio-server tests: in-process first, one real-subprocess smoke.

Strategy:
  * Drive ``McpServer._handle`` directly with decoded JSON-RPC messages — no
    subprocess, no sockets, fully hermetic (fast + deterministic).
  * Drive ``serve()`` through ``io.StringIO`` doubles to prove the framing
    layer: newline-delimited messages, liberal legacy ``Content-Length``
    headers, malformed-line resilience, flush-per-response.
  * ONE guarded subprocess smoke test over real pipes via
    ``sys.executable -m dataworm.cli mcp`` proving end-to-end wiring
    (launch -> handshake -> EOF exit 0). Everything else stays in-process.

Servers are built with ``prefer_rust=False`` so this suite is independent of
the native backend's state on any given machine.
"""

from __future__ import annotations

import io
import json
import queue
import subprocess
import sys
import threading
import time

import pytest

from dataworm import __version__
from dataworm.events import EventBus
from dataworm.mcp import (
    DEFAULT_LEGACY_VERSION,
    LATEST_VERSION,
    MAX_PENDING_NOTIFICATIONS,
    META_SUBSCRIPTION_ID_KEY,
    META_VERSION_KEY,
    SUBSCRIPTION_TYPE_CHANGE,
    McpServer,
    build_tool_catalog,
)

EXPECTED_TOOLS = {
    "worm_crawl",
    "worm_impact",
    "worm_context",
    "worm_neighbors",
    "worm_search",
    "worm_summary",
    "worm_watch",
    "worm_unwatch",
    "worm_changes",
    "worm_plan_edit",
}


# ---- helpers ---------------------------------------------------------------

def build_tree(root):
    """a.py imports b.py; c.md links a.py — same shape as the CLI tests."""
    root.mkdir(parents=True, exist_ok=True)
    (root / "a.py").write_text("import b\n\nprint('a')\n", encoding="utf-8")
    (root / "b.py").write_text("def helper():\n    return 42\n", encoding="utf-8")
    (root / "c.md").write_text("# C\n\nSee [entry](a.py).\n", encoding="utf-8")
    (root / "solo.txt").write_text(
        "unique solitary content with no inbound links whatsoever 7q3f\n",
        encoding="utf-8",
    )
    return root


@pytest.fixture
def srv(tmp_path):
    return McpServer(db_path=str(tmp_path / ".dataworm" / "graph.db"), prefer_rust=False)


def rpc(method, params=None, id=1):
    msg = {"jsonrpc": "2.0", "method": method, "id": id}
    if params is not None:
        msg["params"] = params
    return msg


def notify(method, params=None):
    msg = {"jsonrpc": "2.0", "method": method}
    if params is not None:
        msg["params"] = params
    return msg


def call_tool(server, name, arguments=None, id=99):
    params = {"name": name}
    if arguments is not None:
        params["arguments"] = arguments
    return server._handle(rpc("tools/call", params, id=id))


def tool_payload(response):
    """Unwrap a successful tools/call response into its Core dict."""
    assert "error" not in response, response
    block = response["result"]["content"][0]
    assert block["type"] == "text"
    return json.loads(block["text"])


def serve_lines(server, input_text):
    """Run serve() over StringIO doubles; return the emitted response lines."""
    out = io.StringIO()
    rc = server.serve(stdin=io.StringIO(input_text), stdout=out)
    assert rc == 0
    return [ln for ln in out.getvalue().splitlines() if ln]


# ---- handshake ---------------------------------------------------------------

class TestHandshake:
    def test_initialize_echoes_client_version(self, srv):
        resp = srv._handle(rpc("initialize", {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "test-client", "version": "1.0"},
        }, id=7))
        assert resp["id"] == 7
        result = resp["result"]
        # Echoing the client's proposal guarantees the client can speak it.
        assert result["protocolVersion"] == "2024-11-05"
        assert result["capabilities"]["tools"] == {}
        assert result["serverInfo"]["name"] == "dataworm"
        assert result["serverInfo"]["version"] == __version__

    def test_initialize_default_protocol_version(self, srv):
        resp = srv._handle(rpc("initialize"))
        assert resp["result"]["protocolVersion"] == DEFAULT_LEGACY_VERSION

    def test_initialize_rejects_unknown_legacy_version(self, srv):
        resp = srv._handle(rpc("initialize", {"protocolVersion": "1900-01-01"}, id=8))
        assert resp["error"]["code"] == -32602
        assert "2025-06-18" in resp["error"]["message"]

    def test_initialized_notification_gets_no_response(self, srv):
        assert srv._handle(notify("notifications/initialized")) is None

    def test_unknown_notification_gets_no_response(self, srv):
        assert srv._handle(notify("some/future/notification", {"x": 1})) is None

    def test_full_sequence_over_serve(self, srv):
        lines = [
            json.dumps(rpc("initialize", {"protocolVersion": "2025-06-18"}, id=0)),
            json.dumps(notify("notifications/initialized")),
            json.dumps(rpc("tools/list", {}, id=1)),
        ]
        responses = [json.loads(ln) for ln in serve_lines(srv, "\n".join(lines) + "\n")]
        # Exactly two responses: the initialized notification produced none.
        assert len(responses) == 2
        assert responses[0]["result"]["protocolVersion"] == "2025-06-18"
        names = {t["name"] for t in responses[1]["result"]["tools"]}
        assert EXPECTED_TOOLS <= names


# ---- tools/list ---------------------------------------------------------------

class TestToolsList:
    def test_all_tools_with_valid_input_schema(self, srv):
        resp = srv._handle(rpc("tools/list", id=2))
        tools = {t["name"]: t for t in resp["result"]["tools"]}
        assert set(tools) >= EXPECTED_TOOLS
        for name in EXPECTED_TOOLS:
            spec = tools[name]
            assert spec["description"], name
            schema = spec["inputSchema"]
            assert schema["type"] == "object"
            assert isinstance(schema["properties"], dict)
            assert isinstance(schema["required"], list)
            assert set(schema["required"]) <= set(schema["properties"])

    def test_impact_schema_teaches_required_path(self, srv):
        tools = {t["name"]: t for t in srv._handle(rpc("tools/list"))["result"]["tools"]}
        assert tools["worm_impact"]["inputSchema"]["required"] == ["path"]

    def test_core_is_lazy(self, srv):
        srv._handle(rpc("initialize", id=3))
        srv._handle(rpc("tools/list", id=4))
        assert srv._core is None  # handshake never touches the graph


# ---- tools/call happy paths -----------------------------------------------------

class TestToolCalls:
    def test_crawl_then_impact_blast_radius(self, srv, tmp_path):
        tree = build_tree(tmp_path / "proj")
        res = call_tool(srv, "worm_crawl", {"root": str(tree)}, id=10)
        assert "error" not in res
        crawl = tool_payload(res)
        assert crawl.get("converged") and crawl.get("nodes", 0) > 0

        res = call_tool(srv, "worm_impact", {"path": "b.py"}, id=11)
        assert "error" not in res
        impact = tool_payload(res)
        # a.py imports b.py -> editing b.py hits a.py directly. Entries may be
        # bare ids or {id: ...} dicts depending on the federated resolver.
        direct = [e["id"] if isinstance(e, dict) else e for e in impact["direct"]]
        assert any(str(i).endswith("a.py") for i in direct), impact
        assert impact["total_affected"] >= 1

    def test_empty_blast_radius_means_safe(self, srv, tmp_path):
        tree = build_tree(tmp_path / "proj2")
        call_tool(srv, "worm_crawl", {"root": str(tree)}, id=12)
        impact = tool_payload(call_tool(srv, "worm_impact", {"path": "solo.txt"}, id=13))
        assert impact["direct"] == []
        assert impact["transitive"] == []

    def test_search_maps_query_to_core_text_param(self, srv, tmp_path):
        tree = build_tree(tmp_path / "proj3")
        call_tool(srv, "worm_crawl", {"root": str(tree)}, id=14)
        found = tool_payload(call_tool(srv, "worm_search", {"query": "a.py", "limit": 5}, id=15))
        assert any(hit["id"].endswith("a.py") for hit in found["results"])

    def test_neighbors_and_context_smoke(self, srv, tmp_path):
        tree = build_tree(tmp_path / "proj4")
        call_tool(srv, "worm_crawl", {"root": str(tree)}, id=16)
        nb = tool_payload(call_tool(
            srv, "worm_neighbors", {"path": "b.py", "depth": 1, "edge_type": "references"},
            id=17))
        assert nb, "expected at least a.py as a reference-neighbor of b.py"
        ctx = tool_payload(call_tool(srv, "worm_context", {"path": "a.py"}, id=18))
        assert "impact" in ctx or "links" in ctx

    def test_summary_roundtrip(self, srv, tmp_path):
        tree = build_tree(tmp_path / "proj5")
        call_tool(srv, "worm_crawl", {"root": str(tree)}, id=19)
        summary = tool_payload(call_tool(srv, "worm_summary", {}, id=20))
        assert summary  # non-empty stats bundle


# ---- failure modes -----------------------------------------------------------------

class TestFailureModes:
    def test_unknown_file_yields_isError_result_not_crash(self, srv, tmp_path):
        tree = build_tree(tmp_path / "proj6")
        call_tool(srv, "worm_crawl", {"root": str(tree)}, id=21)
        res = call_tool(srv, "worm_impact", {"path": "nope/does_not_exist.py"}, id=22)
        assert res["result"]["isError"] is True
        block = res["result"]["content"][0]
        assert block["type"] == "text"
        assert "unknown path" in block["text"]
        assert "jsonrpc" not in res["result"]  # still a result, not an envelope error

    def test_unknown_tool_name_invalid_params(self, srv):
        resp = call_tool(srv, "not_a_worm_tool", {})
        assert resp["error"]["code"] == -32602

    def test_missing_tool_argument_invalid_params(self, srv):
        resp = call_tool(srv, "worm_impact", {})
        assert resp["error"]["code"] == -32602
        assert "path" in resp["error"]["message"]

    def test_missing_name_invalid_params(self, srv):
        resp = srv._handle(rpc("tools/call", {"arguments": {}}, id=23))
        assert resp["error"]["code"] == -32602

    def test_unknown_method_minus_32601_echoes_id(self, srv):
        resp = srv._handle(rpc("resources/list", id="abc"))
        assert resp["error"]["code"] == -32601
        assert resp["id"] == "abc"

    def test_malformed_json_line_id_null_then_recovers(self, srv):
        lines = serve_lines(srv, "{this is not json\n" +
                            json.dumps(rpc("initialize", id=42)) + "\n")
        parsed = [json.loads(ln) for ln in lines]
        assert len(parsed) == 2  # loop kept serving after the bad line
        assert parsed[0]["id"] is None
        assert parsed[0]["error"]["code"] == -32700
        assert parsed[1]["result"]["protocolVersion"] == DEFAULT_LEGACY_VERSION

    def test_content_length_framing_accepted(self, srv):
        body = json.dumps(rpc("tools/list", id=5))
        framed = f"Content-Length: {len(body)}\r\n\r\n{body}"
        responses = [json.loads(ln) for ln in serve_lines(srv, framed)]
        assert len(responses) == 1
        assert {t["name"] for t in responses[0]["result"]["tools"]} >= EXPECTED_TOOLS

    def test_non_object_request_rejected(self, srv):
        resp = srv._handle([1, 2, 3])
        assert resp["error"]["code"] == -32600
        assert resp["id"] is None


# ---- live watching -------------------------------------------------------------------

class TestWatching:
    def test_watch_unwatch_roundtrip(self, srv, tmp_path):
        tree = build_tree(tmp_path / "proj7")
        try:
            watched = tool_payload(call_tool(srv, "worm_watch", {"root": str(tree)}, id=30))
            assert watched["ok"] is True
            stopped = tool_payload(call_tool(srv, "worm_unwatch", {"root": str(tree)}, id=31))
            assert stopped["ok"] is True
        finally:
            srv.core.call("unwatch", {"root": str(tree)})


# ---- subprocess smoke -------------------------------------------------------------------

class TestSubprocessSmoke:
    def test_real_stdio_child_handshake_then_eof_exit_zero(self, tmp_path):
        """One true end-to-end pass over OS pipes: launch the CLI subcommand,
        send initialize, close stdin, expect exactly one well-formed response
        and a clean exit-0 on EOF."""
        init_line = json.dumps(rpc("initialize", {"protocolVersion": "2025-06-18"}, id=1)) + "\n"
        try:
            proc = subprocess.run(
                [sys.executable, "-m", "dataworm.cli", "mcp",
                 "--db", str(tmp_path / ".dataworm" / "graph.db"), "--no-rust"],
                input=init_line.encode("utf-8"),
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                timeout=60,
            )
        except OSError as exc:  # pragma: no cover - hostile env only
            pytest.skip(f"cannot spawn subprocess here: {exc}")
        assert proc.returncode == 0, proc.stderr.decode("utf-8", "replace")
        out_lines = [ln for ln in proc.stdout.decode("utf-8").splitlines() if ln.strip()]
        assert len(out_lines) == 1  # notification-free single reply
        resp = json.loads(out_lines[0])
        assert resp["id"] == 1
        assert resp["result"]["protocolVersion"] == "2025-06-18"
        assert resp["result"]["serverInfo"]["name"] == "dataworm"


# ---- modern era (rev 2026-07-28): stateless requests + server/discover ----

def modern(method, params=None, id=1, version=LATEST_VERSION):
    """A rev-2026-07-28 request: version declared via params._meta."""
    params = dict(params or {})
    meta = params.setdefault("_meta", {})
    meta[META_VERSION_KEY] = version
    return rpc(method, params, id=id)


class TestModernEra:
    def test_server_discover_shape(self, srv):
        resp = srv._handle(rpc("server/discover", {
            "_meta": {
                META_VERSION_KEY: LATEST_VERSION,
                "io.modelcontextprotocol/clientInfo": {"name": "c", "version": "1"},
                "io.modelcontextprotocol/clientCapabilities": {},
            },
        }, id=5))
        result = resp["result"]
        assert result["resultType"] == "complete"
        assert result["supportedVersions"] == [LATEST_VERSION]
        assert result["capabilities"]["tools"] == {}
        info = result["_meta"]["io.modelcontextprotocol/serverInfo"]
        assert info == {"name": "dataworm", "version": __version__}
        assert "worm_impact" in result["instructions"]
        assert isinstance(result["ttlMs"], int) and result["ttlMs"] > 0
        assert result["cacheScope"] in ("public", "private")

    def test_discover_works_without_meta(self, srv):
        """Liberal probe: no _meta at all still advertises (era discovery)."""
        resp = srv._handle(rpc("server/discover", {}, id=6))
        assert resp["result"]["supportedVersions"] == [LATEST_VERSION]

    def test_modern_tools_list_carries_cacheable_result(self, srv):
        resp = srv._handle(modern("tools/list", {}, id=7))
        result = resp["result"]
        names = [t["name"] for t in result["tools"]]
        assert len(names) == len(EXPECTED_TOOLS)
        # Deterministic ordering: identical across repeated requests.
        again = srv._handle(modern("tools/list", {}, id=70))
        assert [t["name"] for t in again["result"]["tools"]] == names
        assert result["resultType"] == "complete"
        assert isinstance(result["ttlMs"], int)
        assert result["cacheScope"] in ("public", "private")

    def test_unsupported_modern_version_gets_minus_32022(self, srv):
        resp = srv._handle(modern("tools/list", {}, id=8, version="1900-01-01"))
        error = resp["error"]
        assert error["code"] == -32022
        assert error["data"]["requested"] == "1900-01-01"
        assert LATEST_VERSION in error["data"]["supported"]

    def test_modern_tools_call_round_trip(self, srv, tmp_path):
        tree = tmp_path / "proj"
        tree.mkdir(parents=True)
        (tree / "a.py").write_text("import b", encoding="utf-8")
        (tree / "b.py").write_text("x = 1", encoding="utf-8")
        crawled = tool_payload(call_tool(srv, "worm_crawl", {"root": str(tree)}, id=9))
        assert crawled["nodes"] >= 3
        resp = call_tool(srv, "worm_impact", {"path": "b.py"}, id=10)
        assert resp["result"]["resultType"] == "complete"
        blast = tool_payload(resp)
        # Impact entries are rich objects (id + metadata), not bare strings.
        assert [d["id"] for d in blast["direct"]] == ["a.py"]

    def test_legacy_and_modern_coexist_on_one_process(self, srv):
        """Dual-era: an un-versioned legacy tools/list and a modern one both
        work on the same server instance."""
        legacy = srv._handle(rpc("tools/list", {}, id=11))
        modern_resp = srv._handle(modern("tools/list", {}, id=12))
        assert [t["name"] for t in legacy["result"]["tools"]] == \
               [t["name"] for t in modern_resp["result"]["tools"]]


# ---- Reflex Arc: subscriptions/listen + pushed change notifications ----------
#
# Hermetic by construction: the Core is a fake double exposing a REAL
# dataworm.events.EventBus plus a scripted call(), so bus emissions are driven
# directly without any filesystem watcher, journal, or subprocess. Pushed
# delivery is asserted on the outbound queue (serve() is exercised separately
# over StringIO doubles with a gated stdin).

REPORT_KEYS = {
    "seq", "ts", "kind", "path", "root", "old_hash", "new_hash",
    "refs_lost", "refs_gained", "dangling_now",
    "dependents_before", "dependents_after", "source",
}


def make_report(seq=1, kind="modified", path="a.py"):
    """A pinned change-report dict honoring the journal contract."""
    return {
        "seq": seq,
        "ts": 1_730_000_000.0 + seq,
        "kind": kind,
        "path": path,
        "root": "C:/proj",
        "old_hash": f"old-{seq}",
        "new_hash": f"new-{seq}",
        "refs_lost": [],
        "refs_gained": [],
        "dangling_now": [],
        "dependents_before": [],
        "dependents_after": [],
        "source": "fs_event",
    }


def emit_change(core, report, style="flat"):
    """Simulate Core publishing a change report in each plausible wiring.

    NOTE: ``bus.emit("change", **report)`` as sketched in the brief raises
    TypeError (the report's own ``kind`` collides with emit's positional
    ``kind``), so the real emitter must land in one of these shapes; the MCP
    layer accepts all of them.
    """
    if style == "flat":        # bus.emit(**report) — dict IS the report
        core.bus.emit(**report)
    elif style == "nested":    # bus.emit("change", change=report)
        core.bus.emit("change", change=report)
    elif style == "report":    # bus.emit("change", report=report) — PRODUCTION
        core.bus.emit("change", report=report)  # shape used by core.py
    else:                      # hybrid: envelope kind="change" + report body
        stripped = {k: v for k, v in report.items() if k not in ("seq", "kind")}
        core.bus.emit("change", **stripped)


class FakeChangesCore:
    """Core double: real EventBus + scripted call() (records every op)."""

    def __init__(self, result=None, raise_error=False):
        self.bus = EventBus()
        self.calls = []
        self._result = result if result is not None else {"changes": [], "last_seq": 0}
        self._raise_error = raise_error

    def call(self, method, params=None):
        self.calls.append((method, dict(params or {})))
        if self._raise_error:
            raise RuntimeError(f"unknown op: {method}")
        return self._result


def listening_server(tmp_path, core=None):
    """McpServer wired to a fake Core with the outbound queue active.

    The fake core is injected BEFORE the first listen so the lazy bus hook
    binds the fake's EventBus, never a real Core.
    """
    server = McpServer(db_path=str(tmp_path / ".dataworm" / "graph.db"),
                       prefer_rust=False)
    core = core if core is not None else FakeChangesCore()
    server._core = core
    server._activate_outbound()
    return server, core


def listen(server, types=None, id=77):
    params = {"types": types} if types is not None else {}
    return server._handle(rpc("subscriptions/listen", params, id=id))


def drain(server, timeout=1.0):
    """Collect every queued outbound line (no writer thread is running)."""
    items = []
    q = server._outbound
    first = True
    while True:
        try:
            item = q.get(timeout=timeout)
        except queue.Empty:
            return items
        if item is not None:
            items.append(item)
            if first:
                timeout = min(timeout, 0.05)  # drain fast once data flows
            first = False


def wait_until(predicate, timeout=5.0):
    deadline = time.time() + timeout
    while time.time() < deadline:
        if predicate():
            return True
        time.sleep(0.01)
    return False


class TestSubscriptionsListen:
    def test_listen_acks_complete_with_subscription_id(self, tmp_path):
        server, _ = listening_server(tmp_path)
        result = listen(server, [SUBSCRIPTION_TYPE_CHANGE])["result"]
        assert result["resultType"] == "complete"
        assert isinstance(result["subscriptionId"], str) and result["subscriptionId"]
        assert result["acceptedTypes"] == [SUBSCRIPTION_TYPE_CHANGE]

    def test_unknown_types_are_acked_with_echo(self, tmp_path):
        server, _ = listening_server(tmp_path)
        result = listen(server, ["com.example/whatever"])["result"]
        assert result["resultType"] == "complete"
        assert result["acceptedTypes"] == ["com.example/whatever"]

    def test_default_type_when_types_absent(self, tmp_path):
        server, _ = listening_server(tmp_path)
        result = server._handle(rpc("subscriptions/listen", {}, id=78))["result"]
        assert result["acceptedTypes"] == [SUBSCRIPTION_TYPE_CHANGE]

    def test_discover_advertises_extension_and_instructions(self, srv):
        result = srv._handle(rpc("server/discover", {}, id=79))["result"]
        assert result["capabilities"]["extensions"]["io.dataworm/changes"] == {}
        assert "subscriptions/listen" in result["instructions"]
        assert "worm_changes" in result["instructions"]

    def test_types_must_be_list_of_strings(self, tmp_path):
        server, _ = listening_server(tmp_path)
        resp = listen(server, ["ok", 42])
        assert resp["error"]["code"] == -32602

    def test_unsupported_modern_version_still_rejected_first(self, tmp_path):
        server, _ = listening_server(tmp_path)
        resp = server._handle(modern("subscriptions/listen", {}, id=80,
                                     version="1900-01-01"))
        assert resp["error"]["code"] == -32022

    def test_unsubscribe_roundtrip_then_unknown_id_errors(self, tmp_path):
        server, _ = listening_server(tmp_path)
        sub_id = listen(server)["result"]["subscriptionId"]
        off = server._handle(rpc("subscriptions/listen",
                                 {"unsubscribe": sub_id}, id=81))
        assert off["result"]["unsubscribed"] is True
        bad = server._handle(rpc("subscriptions/listen",
                                 {"unsubscribe": sub_id}, id=82))
        assert bad["error"]["code"] == -32602


class TestPushMechanics:
    def test_bus_change_enqueues_wellformed_notification(self, tmp_path):
        server, core = listening_server(tmp_path)
        sub_id = listen(server)["result"]["subscriptionId"]
        emit_change(core, make_report(seq=5))
        lines = drain(server)
        assert len(lines) == 1
        msg = json.loads(lines[0])
        assert msg["jsonrpc"] == "2.0"
        assert msg["method"] == "notifications/dataworm/change"
        assert msg["params"]["_meta"][META_SUBSCRIPTION_ID_KEY] == sub_id
        assert set(msg["params"]["change"]) == REPORT_KEYS
        assert msg["params"]["change"]["seq"] == 5

    def test_flat_report_kind_is_preserved(self, tmp_path):
        # bus.emit(**report) flattens: the report's own seq/kind must survive.
        server, core = listening_server(tmp_path)
        listen(server)
        emit_change(core, make_report(seq=6, kind="created"))
        (msg,) = [json.loads(ln) for ln in drain(server)]
        assert msg["params"]["change"]["kind"] == "created"
        assert msg["params"]["change"]["seq"] == 6

    def test_no_subscribers_means_silent_drop(self, tmp_path):
        server, core = listening_server(tmp_path)
        emit_change(core, make_report(seq=1))
        assert drain(server, timeout=0.2) == []

    def test_non_matching_subscription_type_gets_nothing(self, tmp_path):
        server, core = listening_server(tmp_path)
        listen(server, ["org.other/unrelated"])
        emit_change(core, make_report(seq=2))
        assert drain(server, timeout=0.2) == []

    def test_unsubscribe_stops_delivery(self, tmp_path):
        server, core = listening_server(tmp_path)
        sub_id = listen(server)["result"]["subscriptionId"]
        emit_change(core, make_report(seq=1))
        assert len(drain(server)) == 1
        server._handle(rpc("subscriptions/listen", {"unsubscribe": sub_id}, id=83))
        emit_change(core, make_report(seq=2))
        assert drain(server, timeout=0.2) == []

    def test_non_change_bus_events_are_ignored(self, tmp_path):
        # Crawl-lifecycle events carry no source; watcher signals use fs_* kinds
        # and no source — neither may masquerade as a change report.
        server, core = listening_server(tmp_path)
        listen(server)
        core.bus.emit("done", converged=True, cycles=1)
        core.bus.emit("fs_modified", path="x.py", root="C:/proj")
        assert drain(server, timeout=0.2) == []

    def test_nested_change_envelope_also_delivered(self, tmp_path):
        # bus.emit("change", change=report): nested wrapping is honoured.
        server, core = listening_server(tmp_path)
        listen(server)
        emit_change(core, make_report(seq=9), style="nested")
        (msg,) = [json.loads(ln) for ln in drain(server)]
        assert msg["params"]["change"]["seq"] == 9

    def test_hybrid_envelope_shape_also_delivered(self, tmp_path):
        # bus.emit("change", **rest_of_report): envelope keeps its own seq and
        # kind="change"; the report body spreads beside it.
        server, core = listening_server(tmp_path)
        listen(server)
        emit_change(core, make_report(seq=11), style="hybrid")
        (msg,) = [json.loads(ln) for ln in drain(server)]
        change = msg["params"]["change"]
        # The hybrid emitter stripped the report's own seq/kind, so the
        # envelope's survive: still a well-formed notification, degraded seq.
        assert isinstance(change["seq"], int)
        assert change["kind"] == "change"
        assert change["path"] == "a.py"
        assert change["source"] == "fs_event"

    def test_production_report_envelope_delivered(self, tmp_path):
        # The REAL core.py shape: bus.emit("change", report=report). This is
        # the regression pin for the live-demo integration bug (payload rode
        # under "report"; the detector only knew "change"/flat/hybrid).
        server, core = listening_server(tmp_path)
        listen(server)
        emit_change(core, make_report(seq=13), style="report")
        (msg,) = [json.loads(ln) for ln in drain(server)]
        change = msg["params"]["change"]
        assert change["seq"] == 13
        assert change["kind"] == "modified"
        assert change["path"] == "a.py"

    def test_backpressure_drops_oldest_keeps_newest(self, tmp_path):
        server, core = listening_server(tmp_path)
        listen(server)
        total = MAX_PENDING_NOTIFICATIONS + 200
        for seq in range(1, total + 1):
            emit_change(core, make_report(seq=seq))
        lines = drain(server, timeout=5.0)
        assert len(lines) == MAX_PENDING_NOTIFICATIONS + 1
        seqs = [json.loads(ln)["params"]["change"]["seq"] for ln in lines]
        assert seqs[0] == total - MAX_PENDING_NOTIFICATIONS  # oldest survivor
        assert seqs[-1] == total                             # newest always kept
        assert seqs == sorted(seqs)                          # FIFO order intact


class GatedReader:
    """stdin double: serves scripted lines, then holds the connection open
    until released — lets the test fire bus events mid-serve deterministically."""

    def __init__(self, lines):
        self._lines = [ln + "\n" for ln in lines]
        self._i = 0
        self.release = threading.Event()

    def readline(self):
        if self._i < len(self._lines):
            line = self._lines[self._i]
            self._i += 1
            return line
        self.release.wait(timeout=10.0)
        return ""  # EOF after release

    def read(self, n=-1):  # Content-Length path never taken here; stay safe
        self.release.wait(timeout=10.0)
        return ""


class TestPushOverServe:
    def test_responses_then_push_share_one_ordered_writer(self, tmp_path):
        server, core = listening_server(tmp_path)
        reader = GatedReader([
            json.dumps(rpc("tools/list", {}, id=1)),
            json.dumps(modern("subscriptions/listen",
                              {"types": [SUBSCRIPTION_TYPE_CHANGE]}, id=2)),
        ])
        out = io.StringIO()
        outcome = {}
        thread = threading.Thread(
            target=lambda: outcome.update(rc=server.serve(stdin=reader, stdout=out)))
        thread.start()
        try:
            assert wait_until(lambda: bool(server._subscriptions))
            emit_change(core, make_report(seq=42))
            assert wait_until(lambda: out.getvalue().count("\n") >= 3)
        finally:
            reader.release.set()
            thread.join(timeout=10.0)
        assert outcome.get("rc") == 0
        parsed = [json.loads(ln) for ln in out.getvalue().splitlines()]
        # Strict FIFO across both producers: two responses, then the push.
        assert [p.get("id") for p in parsed[:2]] == [1, 2]
        push = parsed[2]
        assert "id" not in push
        assert push["method"] == "notifications/dataworm/change"
        assert push["params"]["_meta"][META_SUBSCRIPTION_ID_KEY]
        assert push["params"]["change"]["seq"] == 42


class TestWormChangesTool:
    PINNED = {
        "changes": [
            make_report(seq=3),
            make_report(seq=4, kind="deleted", path="b.py"),
        ],
        "last_seq": 4,
    }

    def test_maps_arguments_to_core_changes_op(self, tmp_path):
        core = FakeChangesCore(result=json.loads(json.dumps(self.PINNED)))
        server, _ = listening_server(tmp_path, core=core)
        payload = tool_payload(call_tool(server, "worm_changes",
                                         {"since_seq": 0, "limit": 10}, id=90))
        assert payload == self.PINNED
        assert core.calls[-1] == ("changes", {"since_seq": 0, "limit": 10})

    def test_optional_arguments_default_to_empty_params(self, tmp_path):
        server, core = listening_server(tmp_path)
        tool_payload(call_tool(server, "worm_changes", {}, id=91))
        assert core.calls[-1] == ("changes", {})

    def test_missing_core_op_yields_iserror_result(self, tmp_path):
        server, _ = listening_server(tmp_path,
                                     core=FakeChangesCore(raise_error=True))
        res = call_tool(server, "worm_changes", {"since_seq": 0}, id=92)
        assert res["result"]["isError"] is True
        block = res["result"]["content"][0]
        assert block["type"] == "text"
        assert "unknown op: changes" in block["text"]

    def test_appended_last_in_catalog_teaching_pagination(self):
        catalog = build_tool_catalog()
        # worm_plan_edit is the newest tool (appended last); worm_changes sits
        # immediately before it. Both keep deterministic positions.
        assert catalog[-1]["name"] == "worm_plan_edit"
        assert catalog[-2]["name"] == "worm_changes"
        schema = catalog[-2]["inputSchema"]
        assert schema["required"] == []
        assert schema["properties"]["since_seq"]["type"] == "integer"
        assert schema["properties"]["limit"]["type"] == "integer"
        assert "last_seq" in catalog[-2]["description"]
        plan = catalog[-1]["inputSchema"]
        assert plan["required"] == ["path", "content"]
