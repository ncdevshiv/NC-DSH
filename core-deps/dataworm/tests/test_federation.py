"""Federation tests: per-directory data, cross-dir linking, reuse, federated
traversal, and local mutation propagation.

Each directory owns its own ``<dir>/.dataworm/graph.db``; fragments link across
directory boundaries via mirrored shadow nodes + cross-link edges; previously-
crawled data is reused rather than re-crawled; queries hop stores; and a local
change in one dir clearly shows its cross-dir impact.
"""

from __future__ import annotations

import time
from pathlib import Path

import pytest

from dataworm.core import Core
from dataworm.models import EdgeType
from dataworm.persist import load_sqlite


# ---- F1: per-dir files ----------------------------------------------------

def test_init_creates_per_dir_graph_files(tmp_path: Path):
    """Running init in a dir with subdirs creates one graph.db per dir."""
    proj = tmp_path / "proj"
    (proj / "sub1").mkdir(parents=True)
    (proj / "sub2").mkdir(parents=True)
    (proj / "top.py").write_text("print('top')\n")
    (proj / "sub1" / "a.py").write_text("print('a')\n")
    (proj / "sub2" / "b.py").write_text("print('b')\n")

    db = str(tmp_path / "g.db")
    core = Core(db_path=db, prefer_rust=True)
    r = core.call("crawl", {"root": str(proj), "max_cycles": 2,
                            "enable_semantic": False, "enable_hashing": False})
    assert r["fragments"] >= 3, f"expected 3 fragments (root + 2 subdirs); got {r}"
    # Each dir has its own .dataworm/graph.db.
    for d in [proj, proj / "sub1", proj / "sub2"]:
        assert (d / ".dataworm" / "graph.db").exists(), f"{d}/.dataworm/graph.db missing"


def test_single_dir_no_subdirs_produces_one_file(tmp_path: Path):
    """A dir with no subdirs produces exactly one graph.db (no fragmentation)."""
    proj = tmp_path / "flat"
    proj.mkdir()
    (proj / "a.py").write_text("print('a')\n")
    (proj / "b.py").write_text("print('b')\n")
    db = str(tmp_path / "g.db")
    core = Core(db_path=db, prefer_rust=True)
    r = core.call("crawl", {"root": str(proj), "max_cycles": 1,
                            "enable_semantic": False, "enable_hashing": False})
    assert r["fragments"] == 1
    assert (proj / ".dataworm" / "graph.db").exists()


# ---- F3: cross-dir link mirroring -----------------------------------------

def test_cross_dir_reference_mirrored_in_both_fragments(tmp_path: Path):
    """A reference from root to a subdir file creates cross-links in both stores."""
    proj = tmp_path / "proj"
    lib = proj / "lib"; lib.mkdir(parents=True)
    (lib / "helper.py").write_text("def help(): return 42\n")
    (lib / "core.py").write_text("import helper\n")
    (proj / "main.py").write_text("import lib.core\n")  # cross-dir: root -> lib

    db = str(tmp_path / "g.db")
    core = Core(db_path=db, prefer_rust=True)
    core.call("crawl", {"root": str(proj), "max_cycles": 2,
                        "enable_semantic": False, "enable_hashing": False})

    root_store = core.store
    lib_store = core._store_for(str(lib))
    assert lib_store is not None

    # Root fragment has the outgoing cross-link (main.py -> lib/core.py shadow).
    root_cross = [e for e in root_store.edges(EdgeType.REFERENCES)
                  if e.attrs.get("cross_dir")]
    assert root_cross, "no outgoing cross-link in root fragment"

    # Lib fragment has the mirrored incoming cross-link.
    lib_cross = [e for e in lib_store.edges(EdgeType.REFERENCES)
                 if e.attrs.get("cross_dir")]
    assert lib_cross, "no incoming cross-link in lib fragment"


# ---- F2: reuse existing crawled data --------------------------------------

def test_reuse_existing_subdir_data_not_re_crawled(tmp_path: Path):
    """Crawling a parent reuses a pre-crawled subdir's graph.db (no re-crawl)."""
    proj = tmp_path / "proj"
    lib = proj / "lib"; lib.mkdir(parents=True)
    (lib / "helper.py").write_text("def help(): return 42\n")
    (lib / "core.py").write_text("import helper\n")
    (proj / "main.py").write_text("print('main')\n")

    # 1. Crawl lib alone — creates lib/.dataworm/graph.db.
    c1 = Core(db_path=str(tmp_path / "lib.db"), prefer_rust=True)
    r1 = c1.call("crawl", {"root": str(lib), "max_cycles": 2,
                           "enable_semantic": False, "enable_hashing": False})
    lib_db = lib / ".dataworm" / "graph.db"
    assert lib_db.exists()
    mtime_before = lib_db.stat().st_mtime

    # 2. Crawl the parent — should REUSE lib's graph.db.
    time.sleep(0.1)
    c2 = Core(db_path=str(tmp_path / "proj.db"), prefer_rust=True)
    r2 = c2.call("crawl", {"root": str(proj), "max_cycles": 2,
                           "enable_semantic": False, "enable_hashing": False})
    mtime_after = lib_db.stat().st_mtime
    assert mtime_before == mtime_after, "lib graph.db was rewritten (not reused)"
    # lib's data is loaded with the same node count.
    lib_store = c2._store_for(str(lib))
    assert lib_store is not None
    assert lib_store.counts()["nodes"] == r1["nodes"]


# ---- F4: federated traversal ----------------------------------------------

def test_federated_impact_hops_stores(tmp_path: Path):
    """Impact of a subdir file returns dependents from another fragment."""
    proj = tmp_path / "proj"
    lib = proj / "lib"; lib.mkdir(parents=True)
    (lib / "core.py").write_text("print('core')\n")
    (proj / "main.py").write_text("import lib.core\n")  # cross-dir

    db = str(tmp_path / "g.db")
    core = Core(db_path=db, prefer_rust=True)
    core.call("crawl", {"root": str(proj), "max_cycles": 2,
                        "enable_semantic": False, "enable_hashing": False})

    # to_id resolves across fragments: "lib/core.py" -> lib's "core.py".
    nid, store = core._federated_resolve("lib/core.py")
    assert nid == "core.py", f"expected core.py, got {nid}"

    # Impact of lib/core.py: main.py (in root) depends on it via cross-link.
    imp = core.call("impact", {"path": "lib/core.py"})
    direct_ids = [d["id"] if isinstance(d, dict) else d for d in imp["direct"]]
    assert any("main.py" in i for i in direct_ids), f"main.py not in direct: {imp['direct']}"
    # The cross-dir dependent is tagged.
    cross = [d for d in imp["direct"] if isinstance(d, dict) and d.get("cross_dir")]
    assert cross, "no cross_dir tag on the cross-fragment dependent"


def test_internal_ref_resolves_within_fragment(tmp_path: Path):
    """A subdir's internal reference resolves within its own fragment."""
    proj = tmp_path / "proj"
    lib = proj / "lib"; lib.mkdir(parents=True)
    (lib / "helper.py").write_text("def help(): return 42\n")
    (lib / "core.py").write_text("import helper\n")
    (proj / "main.py").write_text("print('main')\n")

    db = str(tmp_path / "g.db")
    core = Core(db_path=db, prefer_rust=True)
    core.call("crawl", {"root": str(proj), "max_cycles": 2,
                        "enable_semantic": False, "enable_hashing": False})
    # helper.py is in the lib fragment; core.py references it.
    direct_ids = [d["id"] if isinstance(d, dict) else d
                  for d in core.call("impact", {"path": "helper.py"})["direct"]]
    assert direct_ids == ["core.py"]


# ---- F5: local mutation + cross-link propagation --------------------------

def test_local_mutation_re_crawls_only_changed_fragment(tmp_path: Path):
    """Changing a file in one fragment re-saves only that fragment's graph.db."""
    proj = tmp_path / "proj"
    lib = proj / "lib"; lib.mkdir(parents=True)
    (lib / "core.py").write_text("print('core')\n")
    (proj / "main.py").write_text("print('main')\n")

    db = str(tmp_path / "g.db")
    core = Core(db_path=db, prefer_rust=True)
    core.call("crawl", {"root": str(proj), "max_cycles": 1,
                        "enable_semantic": False, "enable_hashing": False})
    root_db = proj / ".dataworm" / "graph.db"
    lib_db = lib / ".dataworm" / "graph.db"
    root_mtime_before = root_db.stat().st_mtime
    lib_mtime_before = lib_db.stat().st_mtime

    # Mutate a file in lib — only lib's graph.db should be re-saved.
    time.sleep(0.1)
    (lib / "core.py").write_text("print('changed')\n")
    core._on_fs_event("fs_modified", str(lib / "core.py"))
    # Trigger the recrawl synchronously (skip the debounce).
    core._recrawl_incremental()

    lib_mtime_after = lib_db.stat().st_mtime
    root_mtime_after = root_db.stat().st_mtime
    assert lib_mtime_after > lib_mtime_before, "lib graph.db not re-saved after local change"
    assert root_mtime_after == root_mtime_before, "root graph.db was re-saved (should be untouched)"


def test_cross_dir_impact_event_emitted_on_local_change(tmp_path: Path):
    """A local change in a fragment with incoming cross-links emits cross_dir_impact."""
    proj = tmp_path / "proj"
    lib = proj / "lib"; lib.mkdir(parents=True)
    (lib / "core.py").write_text("print('core')\n")
    (proj / "main.py").write_text("import lib.core\n")  # cross-dir: root -> lib

    db = str(tmp_path / "g.db")
    core = Core(db_path=db, prefer_rust=True)
    core.call("crawl", {"root": str(proj), "max_cycles": 2,
                        "enable_semantic": False, "enable_hashing": False})

    events = []
    core.bus.subscribe(lambda ev: events.append(ev.get("kind")))
    # Mutate lib/core.py — lib has an incoming cross-link from root.
    (lib / "core.py").write_text("print('changed')\n")
    core._on_fs_event("fs_modified", str(lib / "core.py"))
    core._recrawl_incremental()

    assert "cross_dir_impact" in events, f"no cross_dir_impact event; saw {set(events)}"


# ---- persistence: per-dir files are loadable independently -----------------

def test_per_dir_file_loadable_independently(tmp_path: Path):
    """Each per-dir graph.db can be loaded on its own (federated independence)."""
    proj = tmp_path / "proj"
    (proj / "sub").mkdir(parents=True)
    (proj / "top.py").write_text("print('top')\n")
    (proj / "sub" / "a.py").write_text("print('a')\n")

    db = str(tmp_path / "g.db")
    core = Core(db_path=db, prefer_rust=True)
    core.call("crawl", {"root": str(proj), "max_cycles": 1,
                        "enable_semantic": False, "enable_hashing": False})

    # Load each fragment independently from its own file.
    root_loaded = load_sqlite(proj / ".dataworm" / "graph.db")
    sub_loaded = load_sqlite(proj / "sub" / ".dataworm" / "graph.db")
    assert root_loaded.counts()["nodes"] > 0
    assert sub_loaded.counts()["nodes"] > 0
    # The sub fragment has a.py; the root fragment has top.py.
    assert sub_loaded.has_node("a.py")
    assert root_loaded.has_node("top.py")
