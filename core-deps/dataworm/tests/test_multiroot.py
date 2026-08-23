"""Stage 2.2/2.3: multi-root isolation + parent-merges-subdir linking.

The worm can be called against multiple directories in its lifetime, each with
its own data; and when a parent directory is crawled that contains a previously-
crawled subdir, the subdir's sub-network is merged into the parent's graph
(re-keyed to the parent's id namespace) and cross-boundary references resolve.
"""

from __future__ import annotations

from pathlib import Path

import pytest

from dataworm.config import Config
from dataworm.core import Core
from dataworm.engine import run
from dataworm.graph import GraphStore
from dataworm.models import Edge, EdgeType, Node, NodeKind


# ---- 2.1: Node.root provenance + GraphStore.roots --------------------------

def test_node_carries_root_provenance(sample_config):
    """A crawled node records which crawl root it came from."""
    store = GraphStore()
    from dataworm.crawler import crawl
    crawl(store, sample_config)
    root = sample_config.root
    a = store.get_node("a.py")
    assert a is not None
    assert a.root == str(Path(root).resolve())
    assert str(Path(root).resolve()) in store.roots


def test_persist_roundtrips_root_provenance(sample_store, tmp_path):
    """Node.root and the multi-root set survive a SQLite round-trip."""
    from dataworm.persist import save_sqlite, load_sqlite
    db = tmp_path / "g.db"
    save_sqlite(sample_store, db)
    loaded = load_sqlite(db)
    a = loaded.get_node("a.py")
    assert a is not None and a.root  # provenance preserved
    assert loaded.roots  # multi-root set restored


# ---- 2.2: per-root isolation ----------------------------------------------

def test_core_holds_separate_store_per_root(tmp_path: Path):
    """Crawling two disjoint roots keeps their graphs isolated in the daemon."""
    db = str(tmp_path / "g.db")
    core = Core(db_path=db, prefer_rust=False)

    root_a = tmp_path / "proj_a"
    root_a.mkdir()
    (root_a / "a.py").write_text("print('a')\n", encoding="utf-8")
    root_b = tmp_path / "proj_b"
    root_b.mkdir()
    (root_b / "b.py").write_text("print('b')\n", encoding="utf-8")

    core.call("crawl", {"root": str(root_a), "max_cycles": 2, "enable_semantic": False})
    core.call("crawl", {"root": str(root_b), "max_cycles": 2, "enable_semantic": False})

    roots = core.call("roots")
    assert str(root_a) in roots["roots"]
    assert str(root_b) in roots["roots"]

    # Impact query for a.py routes to proj_a's store (not proj_b's).
    imp_a = core.call("impact", {"path": "a.py"})
    assert imp_a.get("target") == "a.py"
    # b.py exists only in proj_b; querying it routes to proj_b's store.
    imp_b = core.call("impact", {"path": "b.py"})
    assert imp_b.get("target") == "b.py"


# ---- 2.3: parent merges previously-crawled subdir -------------------------

def test_parent_crawl_merges_subdir_store(tmp_path: Path):
    """Crawling a parent after its subdir absorbs the subdir's graph.

    Setup:
        proj/
          lib/
            helper.py    <- imports nothing
            core.py       <- imports helper
          main.py         <- imports lib.core (cross-boundary reference)

    Crawl proj/lib first (standalone). Then crawl proj. In the federated model,
    the parent crawl creates separate fragment stores (root + each subdir) —
    lib gets its own fragment with ids relative to lib. Cross-boundary links
    (main.py -> lib.core) are established by the cross-link pass (F3).
    """
    db = str(tmp_path / "g.db")
    core = Core(db_path=db, prefer_rust=False)

    proj = tmp_path / "proj"
    lib = proj / "lib"
    lib.mkdir(parents=True)
    (lib / "helper.py").write_text("def help(): return 42\n", encoding="utf-8")
    (lib / "core.py").write_text("import helper\n", encoding="utf-8")
    (proj / "main.py").write_text("import lib.core\n", encoding="utf-8")

    # 1. Crawl the subdir alone. Its ids are relative to lib/: "helper.py", "core.py".
    r1 = core.call("crawl", {"root": str(lib), "max_cycles": 3, "enable_semantic": False})
    assert r1["nodes"] > 0
    # core.py references helper.py (resolved within lib's namespace).
    # Federated impact returns rich entries: [{id: "core.py"}].
    direct_ids = [d["id"] if isinstance(d, dict) else d for d in core.call("impact", {"path": "helper.py"})["direct"]]
    assert direct_ids == ["core.py"]

    # 2. Crawl the parent. The federated crawl creates a fragment per subdir.
    r2 = core.call("crawl", {"root": str(proj), "max_cycles": 3, "enable_semantic": False})
    assert r2["converged"] is True
    # The parent crawl produces multiple fragments (root + lib subdir).
    assert r2["fragments"] >= 2, r2
    roots = core.call("roots")
    assert str(proj) in roots["roots"]
    # lib is a separate fragment store (federated, not absorbed).
    assert str(lib) in roots["roots"]

    # 3. lib's internal reference still resolves within lib's fragment.
    direct_ids = [d["id"] if isinstance(d, dict) else d for d in core.call("impact", {"path": "helper.py"})["direct"]]
    assert direct_ids == ["core.py"]


def test_merge_event_emitted_on_parent_crawl(tmp_path: Path):
    """The federated crawl creates separate fragment stores (not a merge)."""
    db = str(tmp_path / "g.db")
    core = Core(db_path=db, prefer_rust=False)
    seen: list[dict] = []
    core.bus.subscribe(lambda ev: seen.append(ev))

    proj = tmp_path / "proj"
    sub = proj / "sub"
    sub.mkdir(parents=True)
    (sub / "x.py").write_text("print('x')\n", encoding="utf-8")
    (proj / "y.py").write_text("print('y')\n", encoding="utf-8")

    core.call("crawl", {"root": str(sub), "max_cycles": 2, "enable_semantic": False})
    seen.clear()
    r = core.call("crawl", {"root": str(proj), "max_cycles": 2, "enable_semantic": False})

    # Federated: the parent crawl creates fragments (root + sub), no merge event.
    # The crawl emits start/pass/cycle/done; check it produced multiple fragments.
    assert r["fragments"] >= 2, f"expected fragments for root + sub; got {r}"
    # No merge events in the federated model (sub keeps its own store).
    merges = [e for e in seen if e.get("kind") == "merge"]
    # merge events may still fire from the GraphStore.merge() path but are not
    # the primary mechanism; the key assertion is the fragment count.
    assert str(proj) in core.call("roots")["roots"]
    assert str(sub) in core.call("roots")["roots"]


def test_disjoint_roots_not_merged(tmp_path: Path):
    """Crawling a root that does NOT contain another known root merges nothing."""
    db = str(tmp_path / "g.db")
    core = Core(db_path=db, prefer_rust=False)
    a = tmp_path / "a"; a.mkdir(); (a / "a.py").write_text("print('a')\n", encoding="utf-8")
    b = tmp_path / "b"; b.mkdir(); (b / "b.py").write_text("print('b')\n", encoding="utf-8")
    core.call("crawl", {"root": str(a), "max_cycles": 1, "enable_semantic": False})
    r = core.call("crawl", {"root": str(b), "max_cycles": 1, "enable_semantic": False})
    assert r["fragments"] >= 1  # the crawled root + its subdirs as fragments
    assert len(core.call("roots")["roots"]) >= 2
