"""Traversal hostility: link cycles must be recorded but never descended.

These pin the junction/symlink recursion fix in ``crawler.py`` (and the
matching Rust gate). On Windows, NTFS junctions are NOT ``is_symlink()`` —
before the fix, a self-referential junction re-keyed itself onto its target's
node id via ``resolve()`` and recursed until ``RecursionError``.
"""

from __future__ import annotations

import os
import sys

import pytest

from dataworm.config import Config
from dataworm.crawler import MAX_DEPTH
from dataworm.engine import run


def _make_cycle(base):
    """real/a.py + two junctions: loop_a -> base, loop_b -> real."""
    real = base / "real"
    real.mkdir(parents=True, exist_ok=True)
    (real / "a.py").write_text("import os", encoding="utf-8")
    # Junctions need no privilege on Windows; mklink /J equivalent.
    os.system(f'mklink /J "{base / "loop_a"}" "{base}" >nul 2>&1')
    os.system(f'mklink /J "{base / "loop_b"}" "{real}" >nul 2>&1')
    return (
        (base / "loop_a").exists() and (base / "loop_b").is_dir()
    )


@pytest.mark.skipif(sys.platform != "win32", reason="NTFS junctions")
def test_junction_cycle_terminates_and_is_not_descended(tmp_path):
    base = tmp_path / "proj"
    if not _make_cycle(base):
        pytest.skip("junction creation unavailable")

    store = run(Config(root=str(base)), max_cycles=1)

    ids = sorted(n.id for n in store.all_nodes())
    # Termination is the headline assertion (pre-fix: RecursionError).
    assert "loop_a" in ids and "loop_b" in ids
    # Nothing was crawled *beneath* either link segment.
    under = [i for i in ids if i.startswith("loop_a/") or i.startswith("loop_b/")]
    assert under == []
    # The junction did not clobber the root node's fragment-unique id.
    root_node = store.get_node("#root:" + str(base.resolve()).replace("\\", "/"))
    assert root_node is not None
    assert os.path.normcase(root_node.path) == os.path.normcase(str(base))


@pytest.mark.skipif(sys.platform != "win32", reason="NTFS junctions")
def test_junction_subdirs_not_federated(tmp_path):
    from dataworm.crawler import crawl_shallow
    from dataworm.graph import GraphStore

    base = tmp_path / "proj"
    if not _make_cycle(base):
        pytest.skip("junction creation unavailable")

    subdirs = crawl_shallow(GraphStore(), Config(root=str(base)))
    # A junction must never be handed to the fragment crawler: crawling it
    # would crawl its target — possibly this very tree.
    assert all(os.path.normcase(p) != os.path.normcase(str(base)) for p in subdirs)
    assert str((base / "real").resolve()).lower() in {os.path.normcase(p) for p in subdirs} or (
        subdirs == [str(base / "real")]
    )


def test_symlink_pair_cycle_terminates(tmp_path):
    """Unix symlink cycles (and Windows dev-mode symlinks) stay bounded."""
    base = tmp_path / "proj"
    real = base / "real"
    real.mkdir(parents=True)
    (real / "a.py").write_text("x = 1", encoding="utf-8")
    try:
        (base / "link_out").symlink_to(real, target_is_directory=True)
        (base / "self_link").symlink_to(base, target_is_directory=True)
    except OSError:
        pytest.skip("symlink privilege unavailable")

    store = run(Config(root=str(base)), max_cycles=1)
    ids = sorted(n.id for n in store.all_nodes())
    assert "self_link" in ids or "link_out" in ids
    assert not any(i.startswith(("self_link/", "link_out/")) for i in ids)


def test_depth_cap_bounds_deep_trees(tmp_path):
    """300-level nesting terminates at MAX_DEPTH without error."""
    deep = tmp_path / "deep"
    cur = deep
    cur.mkdir(parents=True)
    for i in range(300):
        cur = cur / f"d{i}"
        cur.mkdir()
    (cur / "leaf.py").write_text("x = 1", encoding="utf-8")

    store = run(Config(root=str(deep)), max_cycles=1)
    n = len(store.all_nodes())
    # Root + levels 0..MAX_DEPTH-1 recorded; everything below dropped.
    assert n <= MAX_DEPTH + 1
    assert n >= MAX_DEPTH - 5  # sanity: we really walked most of the chain


def test_both_backends_agree_on_junction_tree(tmp_path):
    """Rust-backed and Python-backed crawls of a link-cycle tree agree:
    same node-id set, no descent beneath any link segment."""
    base = tmp_path / "proj"
    real = base / "real"
    real.mkdir(parents=True)
    (real / "a.py").write_text("import os", encoding="utf-8")
    if sys.platform == "win32":
        if not _make_cycle(base):
            pytest.skip("junction creation unavailable")
    else:
        try:
            (base / "loop_a").symlink_to(base, target_is_directory=True)
        except OSError:
            pytest.skip("symlink privilege unavailable")

    from dataworm.core import Core

    def crawl(prefer_rust: bool, db_name: str):
        core = Core(db_path=str(tmp_path / db_name), prefer_rust=prefer_rust)
        core.call("crawl", {"root": str(base)})
        return sorted(n.id for n in core.store.all_nodes())

    rust_ids = crawl(True, "rust.db")
    py_ids = crawl(False, "py.db")
    assert rust_ids == py_ids
    for ids in (rust_ids, py_ids):
        under = [i for i in ids if i.startswith(("loop_a/", "loop_b/", "self_link/"))]
        assert under == []
