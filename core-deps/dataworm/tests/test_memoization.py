"""Content-addressed memoization (dirty-set recomputation).

Pass outputs (raw reference extraction, simhash fingerprints, embeddings) are
pure functions of ``content_hash`` and are memoized across cycles AND process
restarts. These tests pin the contract:

  correctness  — memoized runs are output-identical to cold runs (both backends)
  efficiency   — a fully-warm second pass performs ZERO reads and ZERO embeds
  persistence  — the memo survives a SQLite round-trip (zero embeds after load)
  invalidation — changed bytes -> new hash -> stale memo NOT trusted
  liveness     — RESOLUTION is never memoized (re-run against the live store)
"""

from __future__ import annotations

import builtins
import copy
import json
import os.path as osp
from pathlib import Path

import pytest

from dataworm.config import Config
from dataworm.core import Core
from dataworm.crawler import crawl
from dataworm.engine import hashing_pass, reference_pass, semantic_pass
from dataworm.extractors import semantic as semantic_mod
from dataworm.extractors.semantic import TfidfEmbedder
from dataworm.graph import GraphStore, PythonGraphStore, _try_rust
from dataworm.models import EdgeType
from dataworm.persist import load_sqlite, save_sqlite


# ---- helpers --------------------------------------------------------------

def fresh_memo() -> dict[str, dict]:
    return {"refs": {}, "simhash": {}, "embed": {}}


def make_store(backend: str, root: str):
    if backend == "rust":
        return GraphStore(root=root)
    return PythonGraphStore(root=root)


def edge_set(store) -> list[tuple]:
    """Fully-specified deterministic edge fingerprint (incl. weights/attrs)."""
    return sorted(
        (e.src, e.dst, e.type.value, round(e.weight, 6),
         json.dumps(e.attrs, sort_keys=True))
        for e in store.all_edges()
    )


def run_content_passes(store, config: Config, memo=None) -> None:
    reference_pass(store, config, memo)
    hashing_pass(store, config, memo)
    semantic_pass(store, config, memo)


def strip_text_cache(store) -> None:
    """Drop the transient per-cycle _text attr so any subsequent read must
    really hit the filesystem — making read-count deltas prove MEMO hits."""
    for n in store.all_nodes():
        n.attrs.pop("_text", None)


class CallCounter:
    """Counts opens under a root + embedder text volume during a window."""

    def __init__(self, root: Path | str):
        # Canonicalize: Windows 8.3 short names (NCDEVS~1) must not break the
        # prefix match against crawler-produced absolute paths.
        self.root = osp.normcase(osp.realpath(str(root)))
        self.opens = 0
        self.embeds = 0

    def install(self, monkeypatch) -> None:
        real_open = builtins.open
        counter = self

        def counting_open(file, *args, **kwargs):
            if osp.normcase(str(file)).startswith(counter.root):
                counter.opens += 1
            return real_open(file, *args, **kwargs)

        monkeypatch.setattr(builtins, "open", counting_open)

    def install_embedder(self, monkeypatch) -> None:
        counter = self

        class CountingTfidf(TfidfEmbedder):
            def embed(self, texts):
                counter.embeds += len(texts)
                return super().embed(texts)

        monkeypatch.setattr(semantic_mod, "get_embedder",
                            lambda cfg: CountingTfidf())


BACKENDS = [
    pytest.param("python", id="python"),
    pytest.param("rust", marks=pytest.mark.skipif(
        _try_rust() is None, reason="rust core not available"), id="rust"),
]


# ---- (a) correctness: memoized == cold -------------------------------------

@pytest.mark.parametrize("backend", BACKENDS)
def test_memoized_runs_are_output_identical_to_cold(backend, sample_root,
                                                    sample_config):
    def build(memo):
        store = make_store(backend, str(sample_root))
        crawl(store, sample_config)
        run_content_passes(store, sample_config, memo)
        return store

    cold = build(fresh_memo())

    # Warm pass 1 populates the shared memo; warm pass 2 is fully all-hit.
    shared = fresh_memo()
    warm1 = build(shared)
    warm2 = build(shared)

    assert warm2.signature() == cold.signature()
    assert edge_set(warm2) == edge_set(cold)
    # The population run must agree too (memo writes never skew output).
    assert edge_set(warm1) == edge_set(cold)


def test_memo_hit_skips_reads_and_embeds_on_second_pass(
        sample_root, sample_config, monkeypatch):
    """Second full pass over an UNCHANGED tree: zero file reads, zero embeds.

    Uses the pure-Python backend (the memoized code paths live there; the
    Rust-native passes are exercised by the identity test above).
    """
    counter = CallCounter(sample_root)
    counter.install(monkeypatch)
    counter.install_embedder(monkeypatch)

    store = PythonGraphStore(root=str(sample_root))
    crawl(store, sample_config)

    # Cold pass: every file read + embedded.
    counter.opens = counter.embeds = 0
    run_content_passes(store, sample_config, store.memo)
    cold_opens, cold_embeds = counter.opens, counter.embeds
    assert cold_opens > 0 and cold_embeds > 0

    # Evict the transient per-cycle text cache so the next read would REALLY
    # hit the disk unless the content-addressed memo answers first.
    strip_text_cache(store)
    counter.opens = counter.embeds = 0
    run_content_passes(store, sample_config, store.memo)
    assert counter.embeds == 0, "warm semantic pass must embed nothing"
    assert counter.opens == 0, "warm passes must not re-read unchanged files"
    assert (counter.opens, counter.embeds) < (cold_opens, cold_embeds)


# ---- (c) persistence: memo survives SQLite ---------------------------------

def test_persistence_roundtrip_zero_embeds(sample_root, sample_config,
                                           tmp_path, monkeypatch):
    store = PythonGraphStore(root=str(sample_root))
    crawl(store, sample_config)
    run_content_passes(store, sample_config, store.memo)
    sig_before = store.signature()
    memo_before = copy.deepcopy(store.memo)

    db = tmp_path / "graph.db"
    save_sqlite(store, db)
    fresh = load_sqlite(db)

    # The memo came back — bit-exact (embed vectors round-trip through JSON).
    assert fresh.memo["refs"].keys() == memo_before["refs"].keys()
    assert fresh.memo["simhash"] == memo_before["simhash"]
    for key, vec in fresh.memo["embed"].items():
        assert {int(k): float(w) for k, w in vec.items()} \
            == memo_before["embed"][key]

    # Drive a fresh Python-backend store with the RESTORED memo over the same
    # unchanged tree: convergence costs zero embeds and lands on the exact
    # same signature.
    py = PythonGraphStore(root=str(sample_root))
    crawl(py, sample_config)
    py.memo.update(fresh.memo)

    counter = CallCounter(sample_root)
    counter.install_embedder(monkeypatch)
    run_content_passes(py, sample_config, py.memo)
    assert counter.embeds == 0
    assert py.signature() == sig_before


# ---- (d) invalidation: changed bytes are re-processed -----------------------

def test_changed_file_invalidates_its_memo_entry(sample_root, sample_config):
    memo = fresh_memo()

    s1 = PythonGraphStore(root=str(sample_root))
    crawl(s1, sample_config)
    run_content_passes(s1, sample_config, memo)
    sig1 = s1.signature()
    assert s1.get_edge("a.py", "b.py", EdgeType.REFERENCES) is not None
    refs_keys_before = set(memo["refs"].keys())

    # Change ONE file's bytes: a.py drops its `import b`. Its sha256 changes,
    # so the stale memo entry (still present under the OLD key) must NOT be
    # trusted; every other file keeps hitting its memo.
    (sample_root / "a.py").write_text("print('no imports now')\n",
                                      encoding="utf-8")

    s2 = PythonGraphStore(root=str(sample_root))
    crawl(s2, sample_config)
    run_content_passes(s2, sample_config, memo)

    assert s2.get_edge("a.py", "b.py", EdgeType.REFERENCES) is None, \
        "changed file's stale memo entry was trusted"
    assert s2.get_edge("b.py", "c.py", EdgeType.REFERENCES) is not None, \
        "unchanged files' memo hits broke"
    assert s2.get_edge("dup2.txt", "dup1.txt", EdgeType.DUPLICATE_OF) is not None
    assert s2.signature() != sig1
    # The new content got its own memo entry (old key left untouched).
    assert len(memo["refs"]) == len(refs_keys_before) + 1


def test_resolution_is_never_memoized(sample_root, tmp_path):
    """Extraction is memoized by content, but RESOLUTION re-runs against the
    current store: a reference that dangles today must resolve once its target
    appears — served entirely from a warm memo."""
    work = tmp_path / "live"
    work.mkdir()
    (work / "a.py").write_text("import b\n\nprint('a')\n", encoding="utf-8")

    memo = fresh_memo()
    cfg = Config(root=str(work))

    s1 = PythonGraphStore(root=str(work))
    crawl(s1, cfg)
    run_content_passes(s1, cfg, memo)
    assert s1.get_edge("a.py", "b.py", EdgeType.REFERENCES) is None  # dangles

    # b.py appears; a.py itself is byte-identical (full memo hit expected).
    (work / "b.py").write_text("print('b')\n", encoding="utf-8")

    s2 = PythonGraphStore(root=str(work))
    crawl(s2, cfg)
    run_content_passes(s2, cfg, memo)
    assert s2.get_edge("a.py", "b.py", EdgeType.REFERENCES) is not None, \
        "memoized extraction froze resolution to a stale store view"


# ---- store-level plumbing ---------------------------------------------------

def test_both_store_backends_carry_a_memo(sample_root):
    for store in (PythonGraphStore(root=str(sample_root)),
                  GraphStore(root=str(sample_root))):
        assert store.memo == {"refs": {}, "simhash": {}, "embed": {}}


# ---- (e) Rust-native memo maps: warm reconvergence, persistence, invalidation

def _flat_project(root: Path) -> Path:
    """Flat single-fragment project (no subdirs): a.py -> b.py; c.py loose."""
    root.mkdir(parents=True, exist_ok=True)
    (root / "a.py").write_text("import b\n\nprint('a')\n", encoding="utf-8")
    (root / "b.py").write_text("print('b')\n", encoding="utf-8")
    (root / "c.py").write_text("print('c')\n", encoding="utf-8")
    return root


def _dependent_set(core: Core, path: str) -> set[str]:
    """Direct + transitive dependents of `path` from the Core impact op."""
    imp = core.call("impact", {"path": path})
    ids = set()
    for entry in imp.get("direct", []) + imp.get("transitive", []):
        ids.add(entry["id"] if isinstance(entry, dict) else entry)
    return ids


@pytest.mark.skipif(_try_rust() is None, reason="rust core not available")
def test_warm_rust_reconvergence_identical(tmp_path):
    """Rust-path crawl twice over an UNCHANGED tree (fresh Core on the same
    DB the second time, warm memo pushed into Rust): identical signature — and
    the first crawl must have grown the server-side memo entries."""
    tree = _flat_project(tmp_path / "proj")
    db = tree / ".dataworm" / "graph.db"

    core1 = Core(db_path=str(db), prefer_rust=True)
    result = core1.call("crawl", {"root": str(tree), "max_cycles": 5})
    assert "error" not in result
    sig1 = core1.store.signature()

    # Server-side memo grew: the Rust passes' native maps were harvested back.
    assert core1.store.memo["refs"], "refs memo empty after rust crawl"
    assert core1.store.memo["simhash"], "simhash memo empty after rust crawl"

    core2 = Core(db_path=str(db), prefer_rust=True)
    result2 = core2.call("crawl", {"root": str(tree), "max_cycles": 5})
    assert "error" not in result2
    assert core2.store.signature() == sig1, \
        "warm rust reconvergence changed the graph"


@pytest.mark.skipif(_try_rust() is None, reason="rust core not available")
def test_rust_persistence_round_trip(tmp_path):
    """The harvested memo survives a SQLite round-trip; a Core reloaded from
    that DB answers summary/impact sanely off the restored graph."""
    tree = _flat_project(tmp_path / "proj")
    db = tree / ".dataworm" / "graph.db"
    core = Core(db_path=str(db), prefer_rust=True)
    result = core.call("crawl", {"root": str(tree), "max_cycles": 5})
    assert "error" not in result
    assert core.store.memo["refs"], "precondition: refs memo populated"

    fresh = load_sqlite(db)
    # The memo came back through persist.save/load (generic store.memo path).
    assert fresh.memo["refs"] or fresh.memo["simhash"] or fresh.memo["embed"]
    for kind in ("refs", "simhash", "embed"):
        assert kind in fresh.memo

    # Reloaded Core sanity: summary works and impact still sees a.py -> b.py.
    core2 = Core(db_path=str(db), prefer_rust=True)
    summary = core2.call("summary")
    assert "error" not in summary
    assert summary.get("edges_references", 0) >= 1
    assert "a.py" in _dependent_set(core2, "b.py")


@pytest.mark.skipif(_try_rust() is None, reason="rust core not available")
def test_invalidation_rust(tmp_path):
    """Changed bytes -> new content_hash -> the stale Rust-side memo entry is
    NOT trusted: references edges follow the rewritten import."""
    tree = _flat_project(tmp_path / "proj")
    db = tree / ".dataworm" / "graph.db"

    core1 = Core(db_path=str(db), prefer_rust=True)
    result = core1.call("crawl", {"root": str(tree), "max_cycles": 5})
    assert "error" not in result
    assert "a.py" in _dependent_set(core1, "b.py")

    # Rewrite a.py: import b -> import c. New sha256 => its stale memo entry
    # must be ignored; b.py loses its dependent, c.py gains one. Unchanged
    # b.py/c.py keep hitting their memo entries.
    (tree / "a.py").write_text("import c\n\nprint('a')\n", encoding="utf-8")

    core2 = Core(db_path=str(db), prefer_rust=True)
    result2 = core2.call("crawl", {"root": str(tree), "max_cycles": 5})
    assert "error" not in result2
    assert "a.py" not in _dependent_set(core2, "b.py"), \
        "changed file's stale rust-side memo entry was trusted"
    assert "a.py" in _dependent_set(core2, "c.py"), \
        "new import edge missing after rust recrawl"
