"""Rust backend parity: the Rust GraphStore + Rust passes (reference, semantic,
all query ops) must produce identical results to the pure-Python fallback
(``PythonGraphStore`` + Python engine passes) on the shared sample tree.

This is the guarantee that moving all compute to Rust changed *where* the work
happens, not *what* it produces — so the daemon, the CLI, and the dashboard
behave identically whether ``dataworm._rust`` is loaded or ``--no-rust`` is set.
"""

from __future__ import annotations

from pathlib import Path

import pytest

from dataworm.config import Config
from dataworm.crawler import crawl
from dataworm.engine import reference_pass, semantic_pass, hashing_pass
from dataworm.graph import GraphStore, PythonGraphStore, _try_rust
from dataworm.models import Edge, EdgeType, Node, NodeKind
from dataworm.query import QueryAPI


pytestmark = pytest.mark.skipif(_try_rust() is None, reason="rust core not available")


@pytest.fixture
def py_store(sample_config):
    """Pure-Python store + passes (the reference baseline)."""
    store = PythonGraphStore(root=str(sample_config.root))
    crawl(store, sample_config)
    reference_pass(store, sample_config)
    hashing_pass(store, sample_config)
    semantic_pass(store, sample_config)
    return store


@pytest.fixture
def rust_store(sample_config):
    """Rust-backed store + the same passes (all running in Rust)."""
    store = GraphStore(root=str(sample_config.root))
    crawl(store, sample_config)
    reference_pass(store, sample_config)
    hashing_pass(store, sample_config)
    semantic_pass(store, sample_config)
    return store


def test_rust_store_is_rust_backed(rust_store):
    """Sanity: the 'rust' fixture actually uses the Rust backend."""
    assert hasattr(rust_store, "_inner"), "expected the Rust-backed store"
    assert not isinstance(rust_store, PythonGraphStore)


def test_counts_parity(py_store, rust_store):
    assert py_store.counts() == rust_store.counts()


def test_signature_parity(py_store, rust_store):
    """The graph fingerprint must be byte-identical between backends."""
    assert py_store.signature() == rust_store.signature()


def test_node_set_parity(py_store, rust_store):
    py_ids = sorted(py_store.node_ids())
    rust_ids = sorted(rust_store.node_ids())
    assert py_ids == rust_ids


def test_all_edges_parity(py_store, rust_store):
    py_edges = sorted((e.src, e.dst, e.type.value, round(e.weight, 6))
                      for e in py_store.edges())
    rust_edges = sorted((e.src, e.dst, e.type.value, round(e.weight, 6))
                        for e in rust_store.edges())
    assert py_edges == rust_edges


@pytest.mark.parametrize("path", ["a.py", "b.py", "c.py", "utils/helper.py",
                                   "docs/readme.md", "dup1.txt"])
def test_impact_parity(py_store, rust_store, path):
    py_api = QueryAPI(py_store)
    rust_api = QueryAPI(rust_store)
    assert py_api.impact_of(path) == rust_api.impact_of(path), path


@pytest.mark.parametrize("path", ["a.py", "b.py", "c.py", "utils/helper.py"])
def test_context_parity(py_store, rust_store, path):
    py_api = QueryAPI(py_store)
    rust_api = QueryAPI(rust_store)
    py_ctx = py_api.context_for(path)
    rust_ctx = rust_api.context_for(path)
    assert py_ctx["node"]["id"] == rust_ctx["node"]["id"]
    assert py_ctx["link_counts"] == rust_ctx["link_counts"]
    assert py_ctx["impact"] == rust_ctx["impact"]


@pytest.mark.parametrize("path,depth", [("a.py", 1), ("b.py", 2), ("c.py", 1)])
def test_neighbors_parity(py_store, rust_store, path, depth):
    py_api = QueryAPI(py_store)
    rust_api = QueryAPI(rust_store)
    py_n = py_api.neighbors(path, depth=depth)
    rust_n = rust_api.neighbors(path, depth=depth)
    assert py_n["neighbors"] == rust_n["neighbors"], (path, depth)


def test_neighbors_with_type_filter_parity(py_store, rust_store):
    py_api = QueryAPI(py_store)
    rust_api = QueryAPI(rust_store)
    for t in [EdgeType.REFERENCES, EdgeType.CONTAINS, EdgeType.DUPLICATE_OF,
              EdgeType.SIMILAR_TO]:
        py_n = py_api.neighbors("b.py", edge_types=[t], depth=2)
        rust_n = rust_api.neighbors("b.py", edge_types=[t], depth=2)
        assert py_n["neighbors"] == rust_n["neighbors"], t


def test_search_parity(py_store, rust_store):
    py_api = QueryAPI(py_store)
    rust_api = QueryAPI(rust_store)
    for q in ["helper", "py", "readme", "dup", "nonexistent_xyz"]:
        assert py_api.search(q) == rust_api.search(q), q


def test_summary_parity(py_store, rust_store):
    py_api = QueryAPI(py_store)
    rust_api = QueryAPI(rust_store)
    py_s = py_api.summary()
    rust_s = rust_api.summary()
    # node_kinds + counts must match (meta may differ in transient keys).
    assert py_s["node_kinds"] == rust_s["node_kinds"]
    for k in ("nodes", "edges", "edges_contains", "edges_references",
              "edges_duplicate_of", "edges_similar_to"):
        assert py_s[k] == rust_s[k], k


def test_to_id_parity(py_store, rust_store, sample_config):
    py_api = QueryAPI(py_store)
    rust_api = QueryAPI(rust_store)
    # Absolute path resolution.
    abs_path = str(Path(sample_config.root) / "a.py")
    assert py_api.to_id(abs_path) == "a.py"
    assert rust_api.to_id(abs_path) == "a.py"
    # Bare id.
    assert py_api.to_id("a.py") == rust_api.to_id("a.py") == "a.py"
    # Suffix match.
    assert py_api.to_id("utils/helper.py") == rust_api.to_id("utils/helper.py")


def test_merge_parity(sample_config, tmp_path):
    """The Rust store's merge must match the Python store's merge."""
    # Build a parent tree containing a sub-dir, and crawl the subdir alone first.
    proj = tmp_path / "merge_proj"
    lib = proj / "lib"
    lib.mkdir(parents=True)
    (lib / "helper.py").write_text("def help(): return 42\n", encoding="utf-8")
    (lib / "core.py").write_text("import helper\n", encoding="utf-8")
    (proj / "main.py").write_text("import lib.core\n", encoding="utf-8")

    def build(store):
        sub_cfg = Config(root=str(lib))
        crawl(store, sub_cfg)
        reference_pass(store, sub_cfg)
        parent_cfg = Config(root=str(proj))
        crawl(store, parent_cfg)
        reference_pass(store, parent_cfg)
        return store

    py = build(PythonGraphStore())
    rust = build(GraphStore())
    assert py.counts() == rust.counts()
    assert py.signature() == rust.signature()
    # Cross-boundary reference resolves in both.
    assert QueryAPI(py).impact_of("lib/core.py")["direct"] == ["main.py"]
    assert QueryAPI(rust).impact_of("lib/core.py")["direct"] == ["main.py"]
