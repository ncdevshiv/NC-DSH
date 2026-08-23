"""Traversal parity: Python backend vs Rust PyO3 backend + watcher safety.

Pins the remaining crawler.py divergences after the Rust symlink/junction
hardening (lexical-path identity, ``symlink_metadata`` descent gate, ONE
bounded follow-stat for link-node attributes, MAX_DEPTH=256):

  (a) a linked FILE is recorded under its own LEXICAL node id with its
      TARGET's size (one bounded follow-stat) — identical in both backends;
  (b) a link whose TARGET lies OUTSIDE the crawl root is still recorded under
      its lexical id by BOTH backends (never silently dropped);
  (c) the polling watcher's snapshot walk TERMINATES on junction cycles
      (pre-fix, ``rglob("*")`` descended through junctions — which are not
      ``is_symlink()`` on Windows — and hung the poller thread forever).

Creating a reparse FILE link needs privileges on many systems (symlink
privilege / dev mode; NTFS junctions cannot target files at all), so every
link-based test skips gracefully when no mechanism works. The two trailing
tests need no links and guard plain-tree behavior on every machine.
"""

from __future__ import annotations

import os
import sys
import threading
from pathlib import Path

import pytest

from dataworm.config import Config
from dataworm.core import Core
from dataworm.crawler import crawl
from dataworm.events import EventBus
from dataworm.graph import GraphStore
from dataworm.watcher import DirectoryWatcher


# ---- helpers ---------------------------------------------------------------

def _reads_through(link: Path, target: Path) -> bool:
    """True if opening the link yields exactly the target's bytes."""
    try:
        return link.read_bytes() == target.read_bytes()
    except OSError:
        return False


def _make_file_link(link: Path, target: Path) -> bool:
    """Create ``link -> target`` pointing at a FILE; True only if it reads.

    Tries, in order: Python symlink (needs dev mode/admin on win32),
    ``mklink`` symlink, and finally an NTFS junction (dir-only on stock NTFS,
    so this usually fails for file targets). Only a link that genuinely reads
    the target's bytes counts.
    """
    if link.exists() or link.is_symlink():
        return False
    try:
        link.symlink_to(target)
        if _reads_through(link, target):
            return True
    except OSError:
        pass
    if sys.platform == "win32":
        rc = os.system(f'cmd /c mklink "{link}" "{target}" >nul 2>&1')
        if rc == 0 and _reads_through(link, target):
            return True
        rc = os.system(f'cmd /c mklink /J "{link}" "{target}" >nul 2>&1')
        if rc == 0 and _reads_through(link, target):
            return True
    try:
        link.unlink(missing_ok=True)
    except OSError:
        pass
    return False


def _collect_sizes(tmp_path: Path, base: Path, prefer_rust: bool):
    """Crawl ``base`` through Core with the requested backend; {id: size}."""
    tag = "rust" if prefer_rust else "py"
    core = Core(db_path=str(tmp_path / f"{tag}.db"), prefer_rust=prefer_rust)
    core.call("crawl", {"root": str(base)})
    return {n.id: n.size for n in core.store.all_nodes()}


# ---- (a) linked-file metadata parity ---------------------------------------

def test_linked_file_metadata_parity_across_backends(tmp_path: Path) -> None:
    """A file-link records its TARGET's size under its own lexical id,
    identically in the pure-Python and Rust-backed crawls."""
    base = tmp_path / "proj"
    real = base / "real"
    real.mkdir(parents=True)
    payload = b"W" * 1234  # distinctive byte length
    (real / "target.bin").write_bytes(payload)
    if not _make_file_link(base / "link.py", real / "target.bin"):
        pytest.skip("file-link creation unavailable (needs symlink privilege)")

    # --- Pure-Python backend first.
    py_store = GraphStore()
    crawl(py_store, Config(root=str(base)))
    py_nodes = {n.id: n for n in py_store.all_nodes()}
    assert py_nodes["real/target.bin"].size == len(payload)
    # The link is its OWN node (lexical id — pre-fix it was re-keyed onto
    # resolve()'d target paths) carrying the TARGET's size (bounded stat).
    assert "link.py" in py_nodes
    assert py_nodes["link.py"].size == len(payload)

    # --- Both backends must agree on ids and on link-entry sizes.
    rust_sizes = _collect_sizes(tmp_path, base, prefer_rust=True)
    py_sizes = _collect_sizes(tmp_path, base, prefer_rust=False)
    assert sorted(rust_sizes) == sorted(py_sizes)
    assert rust_sizes["link.py"] == py_sizes["link.py"] == len(payload)


# ---- (b) out-of-root-targeting link-file -----------------------------------

def test_out_of_root_link_file_recorded_under_lexical_id(tmp_path: Path) -> None:
    """A link under the root pointing OUTSIDE it is still recorded under its
    lexical id by both backends (pre-fix Python silently skipped it because
    resolve() landed outside the root)."""
    outside = tmp_path / "outside"
    outside.mkdir()
    far = outside / "far.bin"
    far.write_bytes(b"F" * 77)

    base = tmp_path / "proj"
    base.mkdir()
    (base / "keep.py").write_text("x = 1\n", encoding="utf-8")

    if not _make_file_link(base / "escape.py", far):
        pytest.skip("file-link creation unavailable (needs symlink privilege)")

    rust_sizes = _collect_sizes(tmp_path, base, prefer_rust=True)
    py_sizes = _collect_sizes(tmp_path, base, prefer_rust=False)
    # Recorded in BOTH id sets, under the same lexical id...
    assert "escape.py" in rust_sizes
    assert "escape.py" in py_sizes
    # ...with matching (target-derived) size.
    assert rust_sizes["escape.py"] == py_sizes["escape.py"] == 77


# ---- (c) watcher snapshot termination ---------------------------------------

@pytest.mark.skipif(sys.platform != "win32", reason="NTFS junctions")
def test_watcher_snapshot_terminates_on_junction_cycle(tmp_path: Path) -> None:
    """_snapshot() must finish on a junction-cycle tree (pre-fix: hang).

    Runs the snapshot on a worker thread joined with a hard 10s timeout; the
    headline assertion is that the thread FINISHED at all.
    """
    base = tmp_path / "proj"
    sub = base / "sub"
    sub.mkdir(parents=True)
    (sub / "a.py").write_text("x = 1\n", encoding="utf-8")
    (base / "__pycache__").mkdir()
    (base / "__pycache__" / "junk.pyc").write_bytes(b"\x00\x01")

    # Self-referential junction: cycle -> base -> cycle -> ...
    rc = os.system(f'cmd /c mklink /J "{base}\\cycle" "{base}" >nul 2>&1')
    if not (base / "cycle").is_dir():
        pytest.skip("junction creation unavailable")

    watcher = DirectoryWatcher(root=base, bus=EventBus())
    outcome: dict = {}

    def run_snapshot() -> None:
        try:
            outcome["snap"] = watcher._snapshot()
        except BaseException as exc:  # surfaced below instead of hanging
            outcome["error"] = exc

    worker = threading.Thread(target=run_snapshot, daemon=True)
    worker.start()
    worker.join(10)
    assert not worker.is_alive(), "_snapshot() hung on the junction cycle"
    assert "error" not in outcome, outcome["error"]
    snap = outcome["snap"]
    assert isinstance(snap, dict)
    # Real files still land in the snapshot…
    assert any(p.endswith("a.py") for p in snap), sorted(snap)
    # …ignored noise does not…
    assert not any("__pycache__" in p for p in snap), sorted(snap)
    # …and nothing beneath the junction segment leaked in.
    assert not any("cycle" in Path(p).parts for p in snap), sorted(snap)


# ---- always-run guards for the rewritten walkers -----------------------------

def test_plain_tree_node_parity_no_links(tmp_path: Path) -> None:
    """With no links involved, both backends agree on node ids AND sizes
    (guards the lexical-identity refactor of crawler.py's file branches)."""
    base = tmp_path / "proj"
    (base / "sub").mkdir(parents=True)
    (base / "top.py").write_text("print('top')\n", encoding="utf-8")
    (base / "sub" / "leaf.py").write_text("x = 1\n", encoding="utf-8")
    big = tmp_path / "big.db"

    core = Core(db_path=str(big), prefer_rust=True)
    core.call("crawl", {"root": str(base)})
    rust_sizes = {n.id: n.size for n in core.store.all_nodes()}

    core_py = Core(db_path=str(tmp_path / "small.db"), prefer_rust=False)
    core_py.call("crawl", {"root": str(base)})
    py_sizes = {n.id: n.size for n in core_py.store.all_nodes()}

    # Core crawls FEDERATED: the root fragment holds top-level files + subdir
    # markers only (sub/leaf.py lives in its own fragment store).
    assert rust_sizes == py_sizes
    assert rust_sizes["top.py"] > 0
    assert "sub" in rust_sizes  # subdir marker recorded by both


def test_snapshot_finds_files_and_honors_ignores(tmp_path: Path) -> None:
    """The rewritten _snapshot keeps the old dict semantics without links:
    recursive discovery, mtime values, ignore-rule honoring."""
    root = tmp_path / "proj"
    (root / "docs").mkdir(parents=True)
    (root / "docs" / "guide.md").write_text("g", encoding="utf-8")
    (root / "app.py").write_text("a", encoding="utf-8")
    (root / "__pycache__").mkdir()
    (root / "__pycache__" / "junk.pyc").write_bytes(b"\x00")

    snap = DirectoryWatcher(root=root, bus=EventBus())._snapshot()
    names = {Path(p).name for p in snap}
    assert {"app.py", "guide.md"} <= names
    assert not any("__pycache__" in p for p in snap), sorted(snap)
    assert all(isinstance(v, float) for v in snap.values())
