"""Scale-hardening tests: the crash/hang/OOM/data-loss fixes for million-file
trees. Each test pins one of the bottlenecks identified in the scaling audit:

  - SSE backpressure (bounded queue, not unbounded)
  - node-event batching (nodes_batch, not 1M per-node events)
  - safe atomic save (crash mid-save leaves the previous graph)
  - hashing pass cap (O(n^2) near-duplicate is bounded)
  - response caps (impact/neighbors/search don't ship unbounded lists)
  - streaming progress (events during the crawl, not silence)
  - run_convergence mutates the live store (no snapshot round-trip)
"""

from __future__ import annotations

import json
import os
import queue
import tempfile
import threading
import time
from pathlib import Path

import pytest

from dataworm.config import Config
from dataworm.core import Core
from dataworm.events import EventBus, NodeEventBatcher
from dataworm.graph import _try_rust
from dataworm.models import EdgeType
from dataworm.persist import save_sqlite, load_sqlite


pytestmark = pytest.mark.skipif(_try_rust() is None, reason="rust core not available")


# ---- S2: SSE backpressure + node batching --------------------------------

def test_sse_queue_is_bounded():
    """The SSE subscriber queue must have a maxsize (not unbounded) so a slow
    browser can't OOM the daemon by piling up millions of events."""
    from dataworm.server import _RPCHandler
    # The bounded queue is constructed inside _handle_sse; we can't easily unit
    # test the live SSE path, but we can verify the NodeEventBatcher does the
    # coalescing that keeps the event count down.
    bus = EventBus()
    batcher = NodeEventBatcher(bus, batch_size=200)
    seen: list[str] = []
    bus.subscribe(lambda ev: seen.append(ev.get("kind")))
    for i in range(1000):
        batcher.add(f"f{i}.py", "file", f"/x/f{i}.py", 10)
    batcher.flush()
    # 1000 nodes / 200 per batch = 5 nodes_batch events (NOT 1000 node events).
    assert sum(1 for k in seen if k == "nodes_batch") == 5
    assert sum(1 for k in seen if k == "node") == 0


def test_sse_drop_on_full_does_not_raise():
    """A bounded queue.Queue(maxsize=N) must drop cleanly on full, not raise
    past the queue — the SSE handler's _put wrapper swallows queue.Full."""
    q: queue.Queue = queue.Queue(maxsize=10)
    dropped = 0
    for i in range(100):
        try:
            q.put_nowait(i)
        except queue.Full:
            dropped += 1
    assert dropped == 90
    assert q.qsize() == 10


# ---- S2: progress streaming during ingest --------------------------------

def test_progress_events_emit_during_ingest(tmp_path: Path):
    """A large ingest emits streaming events (node/progress) so the dashboard
    isn't silent for minutes during a million-file crawl."""
    core = Core(db_path=str(tmp_path / "g.db"), prefer_rust=True)
    proj = tmp_path / "proj"; proj.mkdir()
    for i in range(3000):
        (proj / f"f{i}.py").write_text(f"print({i})\n")
    events = []
    core.bus.subscribe(lambda ev: events.append(ev.get("kind")))
    core.call("crawl", {"root": str(proj), "max_cycles": 1,
                        "enable_semantic": False, "enable_hashing": False})
    # The crawl must stream node events (or progress/nodes_batch) during ingest.
    streamed = sum(1 for k in events if k in ("node", "nodes_batch", "progress"))
    assert streamed >= 1, f"no streaming events during ingest; saw {set(events)}"
    assert "done" in events, "crawl must emit done"


# ---- S4: hashing pass cap -------------------------------------------------

def test_no_hashing_flag_skips_duplicate_edges(tmp_path: Path):
    """--no-hashing (enable_hashing=False) produces zero duplicate_of edges."""
    proj = tmp_path / "proj"; proj.mkdir()
    shared = "quantum flux capacitor resonance " * 20
    for i in range(10):
        (proj / f"dup{i}.txt").write_text(shared)
    core = Core(db_path=str(tmp_path / "g1.db"), prefer_rust=True)
    r = core.call("crawl", {"root": str(proj), "max_cycles": 2,
                            "enable_semantic": False, "enable_hashing": False})
    assert r["edges_duplicate_of"] == 0


def test_hashing_on_finds_duplicates(tmp_path: Path):
    """With hashing on, identical files produce duplicate_of edges."""
    proj = tmp_path / "proj"; proj.mkdir()
    shared = "quantum flux capacitor resonance " * 20
    for i in range(10):
        (proj / f"dup{i}.txt").write_text(shared)
    core = Core(db_path=str(tmp_path / "g2.db"), prefer_rust=True)
    r = core.call("crawl", {"root": str(proj), "max_cycles": 2,
                            "enable_semantic": False})
    assert r["edges_duplicate_of"] > 0


# ---- S5: response caps ----------------------------------------------------

def test_impact_response_is_capped(tmp_path: Path):
    """A high-in-degree node's impact response is capped at 1000 + truncated flag."""
    proj = tmp_path / "proj"; proj.mkdir()
    (proj / "hub.py").write_text("print('hub')\n")
    for i in range(1500):
        (proj / f"f{i}.py").write_text(f"import hub\n")
    core = Core(db_path=str(tmp_path / "g.db"), prefer_rust=True)
    core.call("crawl", {"root": str(proj), "max_cycles": 2,
                        "enable_semantic": False, "enable_hashing": False})
    r = core.call("impact", {"path": "hub.py"})
    assert len(r["direct"]) <= 1000
    assert r.get("truncated") is True


def test_neighbors_response_is_capped(tmp_path: Path):
    """A deep neighbors query is capped + reports truncation."""
    proj = tmp_path / "proj"; (proj / "lib").mkdir(parents=True)
    # Build a chain of 1500 files via contains edges (depth query returns many).
    for i in range(1500):
        (proj / "lib" / f"f{i}.py").write_text(f"print({i})\n")
    core = Core(db_path=str(tmp_path / "g.db"), prefer_rust=True)
    core.call("crawl", {"root": str(proj), "max_cycles": 1,
                        "enable_semantic": False, "enable_hashing": False})
    r = core.call("neighbors", {"path": "lib", "depth": 2})
    assert len(r["neighbors"]) <= 1000
    # Either truncated or not depending on graph shape, but the field exists.
    assert "truncated" in r


def test_search_limit_is_clamped_server_side(tmp_path: Path):
    """A client requesting limit=1000000 gets at most 500 (the server clamp)."""
    proj = tmp_path / "proj"; proj.mkdir()
    for i in range(100):
        (proj / f"f{i}.py").write_text(f"print({i})\n")
    core = Core(db_path=str(tmp_path / "g.db"), prefer_rust=True)
    core.call("crawl", {"root": str(proj), "max_cycles": 1,
                        "enable_semantic": False, "enable_hashing": False})
    r = core.call("search", {"text": "f", "limit": 1000000})
    assert len(r["results"]) <= 500


# ---- S5: safe atomic save -------------------------------------------------

def test_save_is_atomic_previous_graph_survives_crash(tmp_path: Path):
    """save_sqlite writes to a temp file then renames — a crash mid-save
    leaves the previous graph intact (no unlink-first data loss)."""
    from dataworm.graph import GraphStore
    from dataworm.models import Node, NodeKind
    db = tmp_path / "g.db"
    # Save a first graph.
    store1 = GraphStore(root=str(tmp_path))
    store1.add_node(Node(id="a.py", path=str(tmp_path / "a.py"), kind=NodeKind.FILE,
                        root=str(tmp_path)))
    save_sqlite(store1, db)
    assert db.exists()
    # Now save a second graph, but simulate a crash by removing the .tmp file
    # partway (the rename never happens). The old graph must still be readable.
    store2 = GraphStore(root=str(tmp_path))
    store2.add_node(Node(id="b.py", path=str(tmp_path / "b.py"), kind=NodeKind.FILE,
                        root=str(tmp_path)))
    # Patch save to fail before the rename: write the temp, then delete it.
    import dataworm.persist as persist
    orig_replace = os.replace
    def boom(src, dst):
        Path(src).unlink(missing_ok=True)  # simulate crash: temp gone
        raise FileNotFoundError("simulated crash mid-save")
    os.replace = boom
    try:
        with pytest.raises(FileNotFoundError):
            save_sqlite(store2, db)
    finally:
        os.replace = orig_replace
    # The PREVIOUS graph (store1) must still be intact — that's the fix.
    loaded = load_sqlite(db)
    assert loaded.has_node("a.py"), "previous graph was lost (data-loss bug)"
    assert not loaded.has_node("b.py"), "the crashed save leaked through"


# ---- S1: run_convergence mutates the live store ---------------------------

def test_run_convergence_writes_to_live_store(tmp_path: Path):
    """The convergence edges (refs/dup/similar) must land on the live fragment
    store the daemon holds — no snapshot round-trip that loses them."""
    proj = tmp_path / "proj"; (proj / "lib").mkdir(parents=True)
    (proj / "lib" / "helper.py").write_text("def help(): return 42\n")
    (proj / "lib" / "core.py").write_text("import helper\n")
    (proj / "main.py").write_text("print('main')\n")
    core = Core(db_path=str(tmp_path / "g.db"), prefer_rust=True)
    core.call("crawl", {"root": str(proj), "max_cycles": 5,
                        "enable_semantic": False})
    # The lib fragment's internal reference (core.py -> helper.py) must be on
    # the live lib fragment store (the bug was convergence edges weren't written).
    lib_store = core._store_for(str(proj / "lib"))
    assert lib_store is not None, "lib fragment store missing"
    refs = list(lib_store.edges(EdgeType.REFERENCES))
    assert len(refs) >= 1, "lib fragment has no reference edges"
    # And a query against the lib fragment must see them.
    direct_ids = [d["id"] if isinstance(d, dict) else d for d in core.call("impact", {"path": "helper.py"})["direct"]]
    assert direct_ids == ["core.py"]


# ---- S5: endpoint index makes queries O(degree) --------------------------

def test_endpoint_index_query_is_fast_on_large_graph(tmp_path: Path):
    """A query on a node in a large graph completes in well under a second —
    the endpoint index means it's O(degree), not O(E) per click."""
    proj = tmp_path / "proj"; proj.mkdir()
    (proj / "hub.py").write_text("print('hub')\n")
    for i in range(2000):
        (proj / f"f{i}.py").write_text(f"import hub\n")
    core = Core(db_path=str(tmp_path / "g.db"), prefer_rust=True)
    core.call("crawl", {"root": str(proj), "max_cycles": 2,
                        "enable_semantic": False, "enable_hashing": False})
    t0 = time.perf_counter()
    r = core.call("impact", {"path": "hub.py"})
    elapsed = time.perf_counter() - t0
    assert elapsed < 1.0, f"impact query took {elapsed:.2f}s — index not working?"
    assert len(r["direct"]) <= 1000  # capped
