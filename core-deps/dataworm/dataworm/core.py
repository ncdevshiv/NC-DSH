"""Core: the single runner that executes every DataWorm operation.

This is the heart of the v2 architecture. ``Core.call(method, params)`` is the
*only* place ops are executed — the CLI, the JSON-RPC server, the HTTP REST
endpoints, and the SSE event stream all funnel through it.

Execution strategy (the "globally callable" contract):
  1. If the Rust core (``dataworm._rust``) is available and ``prefer_rust`` is
     set, dispatch the op to it in-process. This is the fast path.
  2. Otherwise (or for ops Rust doesn't cover, like ``semantic``), run the
     Python implementation (``engine``/``query``/``persist``).

Both paths honour the same JSON contract — ``{method, params} -> {result|error}``
— so the standalone ``dataworm-core`` binary, the PyO3 module, and the Python
fallback are interchangeable. The daemon holds a warm ``GraphStore`` in memory
across calls; a one-shot CLI invocation loads from SQLite instead.

The Rust core currently implements: ``crawl``, ``hash_pass``, ``extract_refs``,
``impact``, ``signature``, ``simhash``, ``hamming``, ``ping``. Everything else
(``context``, ``neighbors``, ``search``, ``summary``, ``crawl`` orchestration
with references + semantic passes) runs in Python. ``crawl`` is special: Rust
produces the structural snapshot (nodes + contains + content hashes), then
Python runs the reference/semantic passes on top — because those need the
in-memory ``GraphStore`` and the event bus.
"""

from __future__ import annotations

import json
import logging
import os
import sqlite3
import threading
import time
import urllib.request
from pathlib import Path
from typing import Any

from dataworm.config import Config, TEXT_EXTENSIONS
from dataworm.events import EventBus
from dataworm.graph import GraphStore
from dataworm.journal import (
    append_report,
    ensure_journal,
    fetch_since,
    mark_notified,
)
from dataworm.models import Edge, EdgeType, Node, NodeKind
from dataworm.persist import load_sqlite, save_sqlite
from dataworm.query import QueryAPI


def _store_is_rust(store) -> bool:
    """True if `store` is the Rust-backed GraphStore (has `_inner`)."""
    return hasattr(store, "_inner") and hasattr(store, "_rust")

log = logging.getLogger("dataworm.core")

DEFAULT_DB = ".dataworm/graph.db"

# Reflex Arc: per-re-crawl cap on individually journalled change reports.
# Paths beyond the cap are aggregated into a single kind="burst" report.
MAX_CHANGE_REPORTS = 50

# Methods the Rust core can handle entirely on its own (no Python GraphStore
# state needed). Everything else goes through the Python path.
_RUST_NATIVE = {"ping", "simhash", "hamming", "extract_refs", "signature"}


def _try_import_rust():
    """Return the dataworm._rust module, or None if unavailable."""
    try:
        import dataworm._rust as rust  # type: ignore
        # Sanity check: dispatch must exist.
        if not hasattr(rust, "dispatch"):
            return None
        return rust
    except Exception as exc:  # ImportError or build mismatch
        log.debug("rust core unavailable: %s", exc)
        return None


class Core:
    """The single runner. Owns the warm graph + event bus; dispatches ops.

    Parameters
    ----------
    db_path:
        SQLite path for persistence. Loaded on construction if it exists;
        saved after mutating ops (``crawl``).
    prefer_rust:
        If True (default), dispatch native Rust ops to ``dataworm._rust``.
        Set False to force the Python path (``--no-rust``).
    bus:
        Optional event bus. The daemon attaches its own so SSE clients see
        real mutations; a one-shot Core creates a private bus.
    """

    def __init__(
        self,
        db_path: str = DEFAULT_DB,
        prefer_rust: bool = True,
        bus: EventBus | None = None,
    ) -> None:
        self.db_path = Path(db_path)
        self.prefer_rust = prefer_rust
        self.rust = _try_import_rust() if prefer_rust else None
        self.bus = bus if bus is not None else EventBus()
        self.store: GraphStore = GraphStore(bus=self.bus)
        self.api: QueryAPI | None = None
        # Crawl mutation lock: held during _op_crawl's mutation phase so a
        # concurrent dashboard query (served on another thread) sees either the
        # pre-crawl or post-crawl graph, not a half-built one.
        self._crawl_lock = threading.RLock()
        # Per-fragment dangling refs: frag_root -> {node_id: [raw, ...]}. The
        # Rust store's Node objects are transient DTOs (we can't write attrs back
        # to them), so the Rust reference pass's dangling output is captured here
        # per fragment for the cross-link pass. Keyed by fragment root to avoid
        # collisions between fragments that share a relative node id.
        self._frag_dangling: dict[str, dict[str, list[str]]] = {}
        # Filesystem watchers, keyed by root absolute path. The worm's "eyes":
        # each watched root gets one DirectoryWatcher; fs events from it are
        # debounced into an incremental re-crawl (see _schedule_recrawl).
        self._watchers: dict[str, Any] = {}
        self._recrawl_lock = threading.Lock()
        self._recrawl_pending = threading.Event()
        self._recrawl_thread: threading.Thread | None = None
        self._changed_paths: set[str] = set()
        # Raw fs event kinds seen per changed path since the last re-crawl
        # (Reflex Arc needs them to label reports created/modified/deleted/moved).
        self._changed_kinds: dict[str, set[str]] = {}
        # Optional webhook URL for the change-report outbox (best-effort POST
        # of every un-notified journal report after each incremental recrawl).
        # Set via the "configure_webhook" op or a "watch" param.
        self.webhook_url: str | None = None
        # Per-root store registry. Each crawled root gets its own GraphStore
        # (isolated data). When a parent crawl contains a previously-crawled
        # subdir, that subdir's store is merged into the parent's (re-keyed),
        # and the subdir root is recorded as "absorbed" so we don't keep two
        # copies. ``self.store`` stays the "active" store for back-compat with
        # the single-root API/tests.
        self._stores: dict[str, GraphStore] = {}
        self._store_lock = threading.Lock()
        self._load()

    # ---- lifecycle -------------------------------------------------------

    def _load(self) -> None:
        """Load the SQLite DB into the warm store, if present."""
        if self.db_path.exists():
            try:
                self.store = load_sqlite(self.db_path)
                self.store.bus = self.bus
                # Register the loaded store under every root it covers.
                for r in self.store.roots or ([self.store.root] if self.store.root else []):
                    self._stores[r] = self.store
                # Federated fragments: a crawl splits the tree into one db per
                # directory. Load the siblings too, so a one-shot CLI process
                # can resolve paths living in another fragment (the daemon gets
                # this for free because its crawl populates _stores in memory).
                self._load_sibling_fragments()
            except Exception as exc:
                log.warning("failed to load %s: %s; starting empty", self.db_path, exc)
                self.store = GraphStore(bus=self.bus)
        self.api = QueryAPI(self.store)

    def _load_sibling_fragments(self) -> None:
        """Discover + register sibling ``<dir>/.dataworm/graph.db`` fragments.

        Walks the primary root (honouring ignore rules) for other directories'
        fragment databases and loads each not-yet-known one into the multi-root
        registry. Only attempted for the default federated layout — a custom
        ``--out`` path opts out of discovery.
        """
        if self.db_path.name != "graph.db" or self.db_path.parent.name != ".dataworm":
            return  # custom layout: nothing to discover
        primary_root = self.db_path.parent.parent
        if not primary_root.is_dir():
            return

        from dataworm.config import DEFAULT_IGNORE_DIRS

        found: list[Path] = []
        stack = [primary_root]
        while stack:
            current = stack.pop()
            try:
                entries = list(os.scandir(current))
            except OSError:
                continue
            for entry in entries:
                if not entry.is_dir(follow_symlinks=False):
                    continue
                if entry.name in DEFAULT_IGNORE_DIRS or entry.name.startswith("."):
                    continue
                frag_db = Path(entry.path) / ".dataworm" / "graph.db"
                if frag_db.exists():
                    found.append(frag_db)
                else:
                    stack.append(Path(entry.path))

        # Bound the work: a pathological tree shouldn't turn every CLI call
        # into a mass load. The daemon doesn't need this path at all.
        MAX_FRAGMENT_LOADS = 500
        for frag_db in found[:MAX_FRAGMENT_LOADS]:
            frag_root = str(frag_db.parent.parent)
            with self._store_lock:
                if any(Path(r).resolve() == Path(frag_root).resolve()
                       for r in self._stores):
                    continue  # already registered
            try:
                sub_store = load_sqlite(frag_db)
                sub_store.bus = self.bus
                with self._store_lock:
                    for r in sub_store.roots or ([sub_store.root] if sub_store.root else []):
                        self._stores[r] = sub_store
            except Exception as exc:
                log.warning("failed to load fragment %s: %s", frag_db, exc)
        if len(found) > MAX_FRAGMENT_LOADS:
            log.warning(
                "%d sibling fragments found under %s; loaded first %d",
                len(found), primary_root, MAX_FRAGMENT_LOADS,
            )

    def _save(self) -> None:
        """Persist the warm store to SQLite."""
        try:
            save_sqlite(self.store, self.db_path)
        except Exception as exc:
            log.warning("failed to save %s: %s", self.db_path, exc)

    # ---- per-root store selection ---------------------------------------

    def _store_for(self, root: str) -> GraphStore | None:
        """Return the store for ``root`` (exact), else the store whose root is
        the *deepest* known root containing it.

        Paths are canonicalised with ``os.path.realpath`` before comparing so
        Windows 8.3 short names (``C:\\Users\\NCDEVS~1``) match their long form
        (``C:\\Users\\Ncdevshiv``) and symlinked/relative forms line up. Picking
        the deepest match (not the first) matters for nested fragments: a path
        under ``proj/lib`` must resolve to the ``proj/lib`` store, not ``proj``.
        """
        with self._store_lock:
            if root in self._stores:
                return self._stores[root]
            target = Path(os.path.realpath(root))
            best_store: GraphStore | None = None
            best_len = -1
            for known_root, store in self._stores.items():
                known = Path(os.path.realpath(known_root))
                try:
                    target.relative_to(known)
                except ValueError:
                    continue
                if len(str(known)) > best_len:
                    best_len = len(str(known))
                    best_store = store
            return best_store

    # ---- the single entrypoint ------------------------------------------

    def call(self, method: str, params: dict[str, Any] | None = None) -> dict[str, Any]:
        """Dispatch ``method`` with ``params``. Returns a JSON-serialisable dict.

        This is the JSON contract shared by every transport (CLI, JSON-RPC,
        HTTP REST). Returns ``{"error": "..."}`` on failure, never raises.
        """
        params = params or {}
        try:
            handler = _METHODS.get(method)
            if handler is None:
                return {"error": f"unknown method: {method}"}
            return handler(self, params)
        except Exception as exc:
            log.exception("method %s failed", method)
            return {"error": f"{type(exc).__name__}: {exc}"}

    # ---- op handlers -----------------------------------------------------
    # Each takes (self, params) and returns a dict. Native Rust ops delegate
    # to self.rust; the rest use the Python GraphStore/QueryAPI.

    def _op_ping(self, params: dict) -> dict:
        backend = "rust" if self.rust is not None else "python"
        return {"ok": True, "backend": backend, "db": str(self.db_path)}

    # ---- Reflex Arc shared machinery ---------------------------------------

    def _diff_fragment_reports(
        self,
        frag_store,
        frag_root: str,
        pre_states: dict[str, dict],
        source: str,
        kinds_override: dict[str, str] | None = None,
        dang: dict[str, list] | None = None,
    ) -> tuple[list[dict], dict | None]:
        """Diff pre-crawl snapshots against the settled graph into reports.

        Returns (individual_reports, burst_overflow_or_None). When
        ``kinds_override`` is None, kinds are derived from the diff itself
        (vanished→deleted, new→created, hash changed→modified, else skipped).
        """
        dang = dang or {}
        frag_reports: list[dict] = []
        seen_rels: set[str] = set()
        # Post-state for every current file node of this fragment.
        for node in frag_store.nodes(kind=NodeKind.FILE.value):
            if node.attrs.get("external"):
                continue
            seen_rels.add(node.id)
            old = pre_states.get(node.id)
            if old is None:
                if source == "fs_event":
                    continue  # incremental path only reports *requested* paths
                kind = "created"
                old = {"hash": "", "refs": [], "dependents": []}
            else:
                kind = "modified"
            if kinds_override is not None:
                kind = kinds_override.get(node.id, kind)
            if kind == "deleted":
                continue  # handled below via vanished ids
            if old["hash"] == node.content_hash and \
                    kinds_override is None and source == "manual_crawl":
                continue  # unchanged: no report
            post_refs = [e.dst for e in frag_store.out_edges(node.id, EdgeType.REFERENCES)]
            post_deps = [e.src for e in frag_store.in_edges(node.id, EdgeType.REFERENCES)]
            frag_reports.append({
                "ts": time.time(),
                "kind": kind,
                "path": node.id,
                "root": frag_root,
                "old_hash": old["hash"],
                "new_hash": node.content_hash,
                "refs_lost": sorted(set(old["refs"]) - set(post_refs)),
                "refs_gained": sorted(set(post_refs) - set(old["refs"])),
                "dangling_now": sorted(dang.get(node.id, [])),
                "dependents_before": old["dependents"],
                "dependents_after": sorted(post_deps),
                "source": source,
            })
        # Vanished nodes (present before, absent now) => deleted.
        for rel, old in pre_states.items():
            if rel in seen_rels:
                continue
            kind = (kinds_override or {}).get(rel, "deleted")
            if kinds_override is None and kind != "deleted":
                continue
            frag_reports.append({
                "ts": time.time(),
                "kind": "deleted",
                "path": rel,
                "root": frag_root,
                "old_hash": old["hash"],
                "new_hash": "",
                "refs_lost": [],
                "refs_gained": [],
                "dangling_now": [],
                "dependents_before": old["dependents"],
                "dependents_after": [],
                "source": source,
            })
        frag_reports.sort(key=lambda r: r["path"])
        overflow: dict | None = None
        if len(frag_reports) > MAX_CHANGE_REPORTS:
            kept = frag_reports[:MAX_CHANGE_REPORTS]
            overflow_paths = [r["path"] for r in frag_reports[MAX_CHANGE_REPORTS:]]
            log.warning(
                "reflex arc: %d changed paths exceeded the per-crawl report"
                " cap (%d) on %s; aggregating overflow as burst",
                len(frag_reports), MAX_CHANGE_REPORTS, frag_root)
            overflow = {
                "ts": time.time(),
                "kind": "burst",
                "path": "",
                "root": frag_root,
                "old_hash": "",
                "new_hash": "",
                "refs_lost": [],
                "refs_gained": [],
                "dangling_now": [],
                "dependents_before": [],
                "dependents_after": [],
                "paths": overflow_paths,
                "source": source,
            }
            return kept, overflow
        return frag_reports, None

    def _publish_reports(self, reports_to_flush: list[tuple[str, dict]]) -> None:
        """Journal-append reports to their owning fragment DBs, broadcast each
        on the bus with its authoritative seq attached, then drain the webhook
        outbox. Never raises into the caller."""
        touched_dbs: set[str] = set()
        for db_path, report in reports_to_flush:
            try:
                con = sqlite3.connect(db_path)
                try:
                    ensure_journal(con)
                    report["seq"] = append_report(con, report)
                finally:
                    con.close()
            except Exception as exc:
                log.warning("reflex arc: failed to journal %s/%s: %s",
                            report.get("root"), report.get("path"), exc)
                continue  # never raise into the recrawl path
            # NOTE: the pinned report carries its own "kind"/"seq" keys, which
            # would collide with EventBus.emit's positional ``kind`` argument
            # if splatted — so the report rides verbatim under ``report``.
            # Stream consumers filter on event kind == "change".
            self.bus.emit("change", report=report)
            touched_dbs.add(db_path)
        if touched_dbs:
            try:
                self._flush_webhook(touched_dbs)
            except Exception:
                log.exception("webhook flush failed")  # best-effort, always

    def _op_crawl(self, params: dict) -> dict:
        """Build the graph: Rust produces structure, Python adds refs + semantic.

        Rust's ``crawl`` gives us nodes + contains edges + content hashes
        (the expensive I/O + sha256 work). We then run the Python reference
        and semantic passes on the resulting GraphStore, because those need
        the in-memory store and the event bus for live streaming.
        """
        root = params.get("root", "")
        if not root:
            return {"error": "crawl requires 'root'"}
        root = str(Path(root).resolve())
        max_cycles = int(params.get("max_cycles", 5))
        enable_semantic = bool(params.get("enable_semantic", True))
        enable_hashing = bool(params.get("enable_hashing", True))
        threshold = float(params.get("similarity_threshold", 0.35))

        config = Config(
            root=root,
            enable_semantic=enable_semantic,
            enable_hashing=enable_hashing,
            similarity_threshold=threshold,
        )

        self.bus.emit("start", root=root, max_cycles=max_cycles)

        # Trust & Foresight: structured Rust warnings collector for this crawl
        # (surfaced on the crawl result; see _op_crawl_locked return).
        self._rust_warnings: list[dict] = []

        # Hold the crawl lock for the whole mutation phase so concurrent
        # dashboard queries (served on other threads) don't read a half-built
        # graph. RLock so _op_crawl can be re-entered if needed.
        with self._crawl_lock:
            return self._op_crawl_locked(params, root, config, max_cycles,
                                         enable_semantic, threshold)

    def _op_crawl_locked(self, params: dict, root: str, config: Config,
                         max_cycles: int, enable_semantic: bool,
                         threshold: float) -> dict:
        """The crawl body, called under ``self._crawl_lock``.

        Federated: the tree is split into one fragment per directory at the top
        level — the root fragment (files directly in root + subdir markers) plus
        one full-crawl store per immediate subdir. Each fragment is saved to its
        own ``<dir>/.dataworm/graph.db`` so each dir's data lives in that dir.
        """
        from dataworm.crawler import crawl, crawl_shallow
        use_rust = self.rust is not None

        # --- Reflex Arc: PRE-crawl snapshot of every fragment's file nodes ---
        # MUST run before the fresh root store below replaces the old one:
        # (hash + resolved outgoing refs + incoming dependents) per file node,
        # so the post-crawl diff can journal created/modified/deleted with the
        # same pipeline fs-event recrawls use (source="manual_crawl").
        manual_pre: dict[str, dict[str, dict]] = {}
        with self._store_lock:
            pre_stores = list(self._stores.items())
        for frag_root, frag_store in pre_stores:
            pre: dict[str, dict] = {}
            for node in frag_store.nodes(kind=NodeKind.FILE.value):
                if node.attrs.get("external"):
                    continue
                pre[node.id] = self._snapshot_node_state(frag_store, node.id)
            manual_pre[frag_root] = pre

        # --- root fragment: shallow crawl (top-level files + subdir markers) ---
        # The structural walk (traversal + sha256) runs in Rust when available;
        # the Python crawler is the --no-rust parity fallback.
        # Carry the content-addressed memo across the store swap: each crawl
        # builds a fresh GraphStore, but unchanged files' refs/simhash/embed
        # outputs are keyed by content_hash and stay valid. The memo was
        # restored from SQLite at _load and is re-saved by _save_all.
        prev_memo = getattr(self.store, "memo", None) or {}
        self.store = GraphStore(root=root, bus=self.bus)
        self.store.memo.update(prev_memo)
        if use_rust:
            meta = self._rust_crawl(self.store, root, config, shallow=True)
            subdirs = meta.get("subdirs", [])
        else:
            subdirs = crawl_shallow(self.store, config)
        self.api = QueryAPI(self.store)
        with self._store_lock:
            self._stores[root] = self.store

        # --- one fragment per immediate subdir (reuse existing if crawled) ---
        # Each subdir gets its own store. If <subdir>/.dataworm/graph.db already
        # exists (from a prior crawl of that subdir), LOAD it instead of
        # re-crawling — this is the "init y containing pre-crawled x → reuse"
        # path. Otherwise crawl it fully and save to <subdir>/.dataworm/graph.db.
        reused_roots: set[str] = set()
        for sub in subdirs:
            sub_db = Path(sub) / ".dataworm" / "graph.db"
            sub_config = Config(root=sub, enable_semantic=enable_semantic,
                                enable_hashing=config.enable_hashing,
                                similarity_threshold=threshold)
            if sub_db.exists():
                # Reuse: load the existing per-dir graph instead of re-crawling.
                try:
                    sub_store = load_sqlite(sub_db)
                    sub_store.bus = self.bus
                    reused_roots.add(sub)
                    log.info("reused existing fragment %s (%d nodes)",
                             sub_db, sub_store.counts().get("nodes", 0))
                except Exception as exc:
                    log.warning("failed to load %s: %s; re-crawling", sub_db, exc)
                    sub_store = GraphStore(root=sub, bus=self.bus)
                    self._crawl_fragment(sub_store, sub, sub_config, use_rust, crawl)
            else:
                sub_store = GraphStore(root=sub, bus=self.bus)
                self._crawl_fragment(sub_store, sub, sub_config, use_rust, crawl)
            with self._store_lock:
                self._stores[sub] = sub_store

        # --- convergence loop on the root fragment ---
        # (Cross-fragment links are established in F3; for now each fragment's
        # internal references resolve within its own namespace.)
        # --- convergence loop on each fragment ---
        # Each fragment (root + subdirs) gets its own refs/hashing/semantic
        # passes. Cross-fragment links (F3) are established after; for now
        # each fragment's internal references resolve within its own namespace.
        with self._store_lock:
            all_stores = list(self._stores.items())
        self._frag_dangling = {}  # fresh per-fragment dangling for this crawl
        converged = True
        cycles = 0
        for frag_root, frag_store in all_stores:
            frag_config = Config(root=frag_root, enable_semantic=enable_semantic,
                                 enable_hashing=config.enable_hashing,
                                 similarity_threshold=threshold)
            if self.rust is not None and _store_is_rust(frag_store):
                # Seed/harvest the fragment's native memo maps around the
                # convergence loop (same rules as the Python fallback branch).
                frag_store._rust_memos_push()
                result = json.loads(frag_store._inner.run_convergence(
                    max_cycles,
                    frag_config.max_content_bytes,
                    sorted(frag_config.text_extensions),
                    frag_config.max_semantic_nodes,
                    threshold,
                    enable_semantic,
                    frag_config.enable_hashing,
                    frag_config.max_hashing_nodes,
                ))
                frag_store._rust_memos_pull()
                self._frag_dangling[frag_root] = self._replay_convergence_events(
                    result.get("events", []))
                if not result.get("converged", False):
                    converged = False
                cycles = max(cycles, result.get("cycles", 0))
                # Mirror the Python branch's meta bookkeeping (same outer-scope
                # values at this point) so a DB saved after a Rust-backed crawl
                # reloads with summary()["root"] populated.
                frag_store.meta.update({"root": frag_root, "cycles": cycles,
                                        "converged": converged, "max_cycles": max_cycles})
            else:
                # Pure-Python fallback (parity reference).
                from dataworm.engine import reference_pass, semantic_pass, hashing_pass
                prev_sig = None
                frag_dangling: dict[str, list[str]] = {}
                for cycle in range(max_cycles):
                    cycles = cycle + 1
                    for name, fn in [
                        ("references", reference_pass),
                        ("hashing", hashing_pass),
                        ("semantic", semantic_pass),
                    ]:
                        self.bus.emit("pass", name=name, cycle=cycles, status="start")
                        result = fn(frag_store, frag_config)
                        if name == "references" and isinstance(result, dict):
                            frag_dangling = result
                        self.bus.emit("pass", name=name, cycle=cycles, status="end")
                    sig = frag_store.signature()
                    self.bus.emit("cycle", n=cycles, signature=sig)
                    if sig == prev_sig:
                        break
                    prev_sig = sig
                else:
                    converged = False
                frag_store.meta.update({"root": frag_root, "cycles": cycles,
                                        "converged": converged, "max_cycles": max_cycles})
                # Record dangling refs returned by the reference pass for the
                # cross-link pass (works for both store backends).
                self._frag_dangling[frag_root] = frag_dangling

        # --- cross-dir link mirroring (F3) ---
        # After each fragment's internal refs resolve, find references that
        # point to files in OTHER fragments and record shadow nodes + mirrored
        # cross-link edges in both the source and target stores. This is how a
        # change in one dir clearly shows its impact on another dir's data.
        cross_links, _mutated_by_linking = self._link_cross_dir_refs(all_stores, config)

        # --- persist every fragment to its own <dir>/.dataworm/graph.db ---
        # Re-save crawled fragments + any reused fragments that got cross-link
        # shadow nodes. Reused fragments without cross-link mutations are skipped
        # (their on-disk data is unchanged).
        self._save_all(reused_roots)

        # --- Reflex Arc: journal manual-crawl diffs (source="manual_crawl") ---
        # Same pipeline as fs-event recrawls: created/modified/deleted per
        # fragment, cap+burst, journal, bus, webhook. Unchanged files produce
        # no reports, so a redundant re-crawl appends nothing.
        manual_reports: list[tuple[str, dict]] = []
        for frag_root, frag_store in all_stores:
            if frag_root in reused_roots:
                # Reuse contract: a loaded-not-recrawled fragment's file is
                # NEVER touched (federation test pins the mtime). Its state
                # was already current when loaded, so there is nothing to
                # diff and nothing to journal.
                continue
            pre = manual_pre.get(frag_root) or {}
            db_path = str(Path(frag_root) / ".dataworm" / "graph.db")
            dang = self._frag_dangling.get(frag_root, {})
            frag_reports, burst = self._diff_fragment_reports(
                frag_store, frag_root, pre,
                source="manual_crawl", dang=dang,
            )
            for report in frag_reports:
                manual_reports.append((db_path, report))
            if burst is not None:
                manual_reports.append((db_path, burst))
        if manual_reports:
            self._publish_reports(manual_reports)

        counts = self._aggregate_counts()
        self.bus.emit("done", converged=converged, cycles=cycles,
                       counts=counts, root=root)
        return {
            "converged": converged,
            "cycles": cycles,
            "root": root,
            "fragments": len(all_stores),
            "warnings": self._rust_warnings[:200],
            **counts,
        }

    def _aggregate_counts(self) -> dict:
        """Sum counts across all fragment stores (the daemon's whole federation)."""
        totals = {"nodes": 0, "edges": 0, "edges_contains": 0,
                  "edges_references": 0, "edges_duplicate_of": 0,
                  "edges_similar_to": 0}
        with self._store_lock:
            stores = list(self._stores.values())
        seen: set[int] = set()
        for store in stores:
            if id(store) in seen:
                continue
            seen.add(id(store))
            c = store.counts()
            for k in totals:
                totals[k] += c.get(k, 0)
        return totals

    def _link_cross_dir_refs(self, all_stores, config: Config) -> tuple[int, set[str]]:
        """Find references that cross fragment boundaries and mirror them.

        For each file node's references, if a reference resolves to a file in a
        *different* fragment, record:
          - in the SOURCE store: a shadow node (id=abs target path,
            attrs={external, target_dir}) + a cross-link edge (references,
            attrs={cross_dir, target_dir}).
          - in the TARGET store: a shadow node (id=abs source path,
            attrs={external, source_dir}) + a cross-link edge (references,
            attrs={cross_dir, source_dir}) — the incoming mirror.

        Returns ``(cross_count, mutated_roots)`` where ``mutated_roots`` is
        the set of fragment roots whose stores actually gained shadow nodes or
        cross-link edges (both directions). Callers persist exactly those
        fragments so mirrored links survive a restart.
        """
        from dataworm.extractors import references as refs_mod
        from dataworm.models import Node, NodeKind

        # Build an absolute-path -> (store, node_id) index across all fragments.
        path_index: dict[str, tuple[GraphStore, str]] = {}
        for _frag_root, store in all_stores:
            for node in store.all_nodes():
                if node.kind == NodeKind.FILE and node.path:
                    path_index[node.path] = (store, node.id)

        cross_count = 0
        # Fragment roots whose stores this pass mutated (source AND target).
        mutated_roots: set[str] = set()
        for frag_root, store in all_stores:
            frag_dangling = self._frag_dangling.get(frag_root, {})
            for node in store.nodes(kind=NodeKind.FILE.value):
                # Use the DANGLING references already recorded by reference_pass
                # (refs that didn't resolve within this fragment). These are the
                # only candidates for cross-dir resolution — re-reading every
                # file's text here would duplicate the convergence pass's work
                # and is the bottleneck on large trees (was O(total_files) reads).
                # Rust path: per-fragment cache; Python path: node attrs.
                dangling = frag_dangling.get(node.id) or node.attrs.get("dangling", [])
                if not dangling:
                    continue  # no unresolved refs — skip this file entirely
                for raw in dangling:
                    # Try to resolve the reference to an absolute path in another fragment.
                    target_abs = self._resolve_cross_dir(node, raw, path_index, store)
                    if target_abs is None:
                        continue
                    target_store, target_id = path_index[target_abs]
                    if target_store is store:
                        continue  # same fragment — internal ref, not cross-dir
                    # Source shadow + outgoing cross-link.
                    src_shadow_id = node.path  # abs path as the shadow id
                    if not store.has_node(src_shadow_id) or not store.get_edge(node.id, target_abs, EdgeType.REFERENCES):
                        store.add_node(Node(
                            id=target_abs, path=target_abs, kind=NodeKind.FILE,
                            root=frag_root, attrs={"external": True, "target_dir": target_store.root},
                        ))
                        store.add_edge(Edge(
                            src=node.id, dst=target_abs, type=EdgeType.REFERENCES,
                            attrs={"cross_dir": True, "target_dir": target_store.root},
                        ))
                        mutated_roots.add(frag_root)
                    # Target shadow + incoming cross-link (the mirror).
                    if not target_store.has_node(src_shadow_id):
                        target_store.add_node(Node(
                            id=src_shadow_id, path=node.path, kind=NodeKind.FILE,
                            root=target_store.root, attrs={"external": True, "source_dir": frag_root},
                        ))
                        mutated_roots.add(target_store.root)
                    if not target_store.get_edge(src_shadow_id, target_id, EdgeType.REFERENCES):
                        target_store.add_edge(Edge(
                            src=src_shadow_id, dst=target_id, type=EdgeType.REFERENCES,
                            attrs={"cross_dir": True, "source_dir": frag_root},
                        ))
                        mutated_roots.add(target_store.root)
                    cross_count += 1
        if cross_count:
            self.bus.emit("cross_links", count=cross_count)
        return cross_count, mutated_roots

    def _resolve_cross_dir(self, node, raw: str,
                           path_index: dict, source_store) -> str | None:
        """Resolve a reference to an absolute path in a DIFFERENT fragment.

        Tries the reference as a path relative to the node's directory, then
        checks if the resulting absolute path exists in another fragment's
        path_index. Returns the absolute path or None.
        """
        from pathlib import Path
        node_dir = Path(node.path).parent
        # Strip anchors/query.
        raw = raw.split("#", 1)[0].split("?", 1)[0]
        if not raw or raw.startswith(("http://", "https://", "mailto:", "data:")):
            return None
        # Try as a relative path from the node's directory.
        candidates = []
        if raw.startswith(".") or "/" in raw or "\\" in raw:
            # Path-like reference (./x, ../x, x/y).
            cand = (node_dir / raw).resolve()
            candidates.append(str(cand))
            # Also try with common extensions.
            from dataworm.extractors.references import _RESOLVE_EXTS
            for ext in _RESOLVE_EXTS:
                candidates.append(str(cand) + ext)
        else:
            # Bare module name — try as a sibling file.
            for ext in (".py", ".js", ".ts", ".md", ".json"):
                candidates.append(str(node_dir / (raw + ext)))
            # Also try as a path into subdirs (e.g. "lib.core" -> lib/core.py).
            mod_path = raw.replace(".", "/")
            candidates.append(str(node_dir / (mod_path + ".py")))
        for cand in candidates:
            if cand in path_index:
                store, _ = path_index[cand]
                if store is not source_store:
                    return cand
        return None

    def _save_all(self, reused_roots: set[str] | None = None) -> None:
        """Persist every fragment store to its own ``<dir>/.dataworm/graph.db``.

        This is the federated save: each directory's data lives in that
        directory, so a mutation in one dir rewrites only that dir's file.
        Fragments in ``reused_roots`` were loaded from disk; skip re-saving
        them UNLESS they gained cross-link shadow nodes (external nodes).
        """
        reused = reused_roots or set()
        with self._store_lock:
            items = list(self._stores.items())
        seen: set[int] = set()
        for frag_root, store in items:
            if id(store) in seen:
                continue
            seen.add(id(store))
            if frag_root in reused:
                # Only re-save if cross-linking added shadow nodes to it.
                has_external = any(n.attrs.get("external") for n in store.all_nodes())
                if not has_external:
                    continue  # loaded from disk, unchanged — don't re-write
            db_path = Path(frag_root) / ".dataworm" / "graph.db"
            try:
                save_sqlite(store, db_path)
            except Exception as exc:
                log.warning("failed to save %s: %s", db_path, exc)

    def _replay_convergence_events(self, events: list) -> dict[str, list[str]]:
        """Replay a Rust run_convergence event log onto the bus.

        The Rust loop runs entirely in Rust (zero Python crossings per cycle);
        it returns an event log whose shapes mirror the engine's bus emissions.
        We translate each into a bus.emit so the live dashboard animates the
        same way it did when the loop ran in Python.

        Returns the merged dangling-ref map ({node_id: [raw, ...]}) collected
        from the reference-pass results, so the caller can record it per
        fragment for the cross-link pass.
        """
        dangling_out: dict[str, list[str]] = {}
        for ev in events:
            kind = ev.get("kind")
            if kind == "start":
                self.bus.emit("start", root=ev.get("root", ""), max_cycles=ev.get("max_cycles", 0))
            elif kind == "pass":
                self.bus.emit("pass", name=ev.get("name", ""), cycle=ev.get("cycle", 0),
                              status=ev.get("status", ""))
            elif kind == "refs_result":
                data = ev.get("data", {})
                if data.get("removed") and self.bus is not None:
                    self.bus.emit("reset_dim", edge_type=EdgeType.REFERENCES.value,
                                  removed=data["removed"])
                if self.bus is not None:
                    for src, dst in data.get("added_edges", []):
                        self.bus.emit("edge", src=src, dst=dst,
                                      edge_type=EdgeType.REFERENCES.value, weight=1.0)
                # Collect dangling refs for the cross-link pass (F3). The Rust
                # store's Node objects are transient DTOs — we can't write attrs
                # back to them, so we return them for per-fragment recording.
                dangling_map = data.get("dangling", {})
                if isinstance(dangling_map, dict):
                    for node_id, dangling in dangling_map.items():
                        dangling_out[node_id] = dangling
            elif kind == "hash_result":
                data = ev.get("data", {})
                if data.get("removed") and self.bus is not None:
                    self.bus.emit("reset_dim", edge_type=EdgeType.DUPLICATE_OF.value,
                                  removed=data["removed"])
                if self.bus is not None:
                    for src, dst in data.get("added_edges", []):
                        self.bus.emit("edge", src=src, dst=dst,
                                      edge_type=EdgeType.DUPLICATE_OF.value, weight=1.0)
            elif kind == "sem_result":
                data = ev.get("data", {})
                if self.bus is not None:
                    self.bus.emit("reset_dim", edge_type=EdgeType.SIMILAR_TO.value,
                                  removed=data.get("removed", 0))
                    for entry in data.get("added_edges", []):
                        src, dst = entry[0], entry[1]
                        w = entry[2] if len(entry) > 2 else 1.0
                        self.bus.emit("edge", src=src, dst=dst,
                                      edge_type=EdgeType.SIMILAR_TO.value, weight=w)
            elif kind == "cycle":
                self.bus.emit("cycle", n=ev.get("n", 0), signature=ev.get("signature", ""))
        return dangling_out

    def _existing_map(self, store) -> dict:
        """id -> {mtime, size, hash} for file nodes with a cached content hash.

        Passed to the Rust crawl so unchanged files (same mtime+size) reuse
        their hash instead of being re-read/re-hashed — the incremental re-crawl
        speedup, mirroring the Python crawler's mtime cache.
        """
        ex: dict[str, dict] = {}
        for node in store.all_nodes():
            if node.kind == NodeKind.FILE and node.content_hash:
                ex[node.id] = {"mtime": node.mtime, "size": node.size,
                               "hash": node.content_hash}
        return ex

    def _rust_crawl(self, store, root: str, config: Config, shallow: bool) -> dict:
        """Run the structural crawl (traversal + sha256) in Rust and apply the
        resulting snapshot to ``store``. Returns the snapshot meta (which carries
        ``subdirs`` when ``shallow``). Raises on a Rust-side error so callers can
        fall back if they wish."""
        snap = self.rust.dispatch("crawl", {
            "root": root,
            "shallow": shallow,
            "ignore_dirs": sorted(config.ignore_dirs),
            "ignore_globs": list(config.ignore_globs),
            "text_extensions": sorted(config.text_extensions),
            "max_content_bytes": config.max_content_bytes,
            "existing": self._existing_map(store),
        })
        if isinstance(snap, dict) and "error" in snap:
            raise RuntimeError(f"rust crawl failed: {snap['error']}")
        # Structured warnings (Trust & Foresight): traversal/stat/hash failures
        # are recorded instead of silently producing empty hashes.
        for w in (snap.get("warnings") or []) if isinstance(snap, dict) else []:
            self._rust_warnings.append(w)
            log.warning("rust crawl warning: %s %s: %s",
                        w.get("op"), w.get("path"), w.get("error"))
        self._apply_crawl_snapshot(store, snap)
        return snap.get("meta", {}) if isinstance(snap, dict) else {}

    def _apply_crawl_snapshot(self, store, snap: dict) -> None:
        """Apply a Rust crawl snapshot (nodes + ``contains`` edges) to ``store``.

        Works for both a fresh store (full crawl) and a populated one
        (incremental re-crawl): structural ``contains`` edges are rebuilt, stale
        non-external nodes are dropped, and snapshot nodes are upserted. External
        (cross-dir shadow) nodes are preserved. Node events are batched so a huge
        ingest streams a few ``nodes_batch`` events, not one per node.
        """
        from dataworm.events import NodeEventBatcher
        root = snap.get("root", "")
        snap_nodes = snap.get("nodes", [])
        snap_ids = {nd["id"] for nd in snap_nodes}

        store.root = root
        if root:
            store.roots.add(root)
        # 1. Rebuild structural edges from scratch.
        store.clear_edges(EdgeType.CONTAINS)
        # 2. Drop stale structural nodes (gone from the tree); keep externals.
        # One batch call: per-node removal costs an O(V) index cleanup each on
        # the Rust store, which goes quadratic on mass deletes.
        stale_ids = [
            node.id for node in store.all_nodes()
            if not node.attrs.get("external") and node.id not in snap_ids
        ]
        if stale_ids:
            store.remove_nodes_batch(stale_ids)
        # 3. Upsert snapshot nodes with the bus detached; batch-emit ourselves.
        store.bus = None
        batcher = NodeEventBatcher(self.bus, batch_size=200)
        for nd in snap_nodes:
            node = Node(
                id=nd["id"],
                path=nd["path"],
                kind=NodeKind(nd["kind"]),
                size=nd.get("size", 0),
                mtime=nd.get("mtime", 0.0),
                mime=nd.get("mime", ""),
                content_hash=nd.get("content_hash", ""),
                root=root,
                attrs=nd.get("attrs", {}),
            )
            store.add_node(node)  # no per-node bus event (bus=None)
            batcher.add(node.id, node.kind.value, node.path, node.size)
        batcher.flush()
        # 4. Re-attach the bus; add contains edges (each emits an edge event).
        store.bus = self.bus
        for ed in snap.get("edges", []):
            store.add_edge(Edge(
                src=ed["src"],
                dst=ed["dst"],
                type=EdgeType(ed["edge_type"]),
                weight=ed.get("weight", 1.0),
                attrs=ed.get("attrs", {}),
            ))

    def _crawl_fragment(self, store, root: str, config: Config,
                        use_rust: bool, py_crawl) -> None:
        """Full (non-shallow) structural crawl of one fragment, Rust-first with
        a Python fallback."""
        if use_rust:
            try:
                self._rust_crawl(store, root, config, shallow=False)
                return
            except Exception as exc:
                log.warning("rust crawl of %s failed (%s); using python", root, exc)
        py_crawl(store, config)

    def _op_plan_edit(self, params: dict) -> dict:
        """What-if edit simulator: the blast radius of PROPOSED content.

        Pure computation over the current graph — never writes disk, never
        mutates the graph. Resolves the references the proposed content WOULD
        have, diffs them against the file's current links, and runs the
        duplication radar (exact by sha256, near by simhash against the
        memoized fingerprints) so an agent can preview an edit before making
        it. Works for brand-new paths too (current_hash == "").
        """
        import hashlib as _hl

        from dataworm.extractors import hashing as _hashing
        from dataworm.extractors import references as _references
        from dataworm.models import Node as _Node, NodeKind as _NodeKind

        path = str(params.get("path", ""))
        content = params.get("content")
        if not path or not isinstance(content, str):
            return {"error": "plan_edit requires 'path' and string 'content'"}

        with self._crawl_lock:
            node_id, store = self._federated_resolve(path)
            if store is None:
                # Planning a NEW file: resolve refs against the active graph.
                store = self.store if self.store is not None else None
                if store is None and self.api is not None:
                    store = self.api.store
                if store is None:
                    return {"error": "no graph loaded; run crawl first"}
            pseudo_id = (node_id or path.replace("\\", "/")).replace("\\", "/")
            pseudo = _Node(id=pseudo_id, path="", kind=_NodeKind.FILE)

            new_hash = _hl.sha256(content.encode("utf-8")).hexdigest()
            current_hash = ""
            if node_id:
                node = store.get_node(node_id)
                current_hash = node.content_hash if node else ""
            unchanged = bool(node_id) and current_hash == new_hash

            # Would-be outgoing links: extraction + resolution against the
            # CURRENT graph (resolution always re-runs; only extraction is
            # ever cached anywhere).
            raw_refs = _references.extract_raw_references(pseudo, content)
            resolved: set[str] = set()
            dangling: list[str] = []
            for raw in raw_refs:
                target = _references.resolve_reference(store, pseudo, raw)
                if target and target != pseudo.id:
                    resolved.add(target)
                elif not target:
                    dangling.append(raw)

            current_targets: set[str] = set()
            dependents_count = 0
            if node_id:
                current_targets = {
                    e.dst for e in store.out_edges(node_id, EdgeType.REFERENCES)
                }
                dependents_count = sum(
                    1 for _ in store.in_edges(node_id, EdgeType.REFERENCES)
                )

            # Duplication radar: exact twin by hash, near twin by simhash.
            exact_duplicate_of = ""
            near_duplicates: list[dict] = []
            others: list[tuple[str, str]] = []  # (id, content_hash)
            for n in store.nodes(kind=_NodeKind.FILE.value):
                if n.id == pseudo.id:
                    continue
                others.append((n.id, n.content_hash))
                if not exact_duplicate_of and n.content_hash == new_hash:
                    exact_duplicate_of = n.id
            if not unchanged:
                fp_new = _hashing.simhash(content)
                if fp_new:
                    # Memo simhash entries are keyed by CONTENT HASH, not node
                    # id — map hashes back to ids so reports name real files.
                    hash_to_id: dict[str, str] = {}
                    for oid, ch in others:
                        if ch and ch not in hash_to_id:
                            hash_to_id[ch] = oid
                    memo = getattr(store, "memo", None) or {}
                    raw_fps: dict[str, int] = {
                        k: v for k, v in (memo.get("simhash") or {}).items()
                        if isinstance(v, int) and v
                    }
                    if len(raw_fps) < 2:  # cold memo: compute fresh, bounded
                        raw_fps = {}
                        budget = 5000
                        for oid, _ch in others:
                            if budget <= 0:
                                break
                            onode = store.get_node(oid)
                            if onode is None or onode.size > 2 * 1024 * 1024:
                                continue
                            suffix = "." + oid.rsplit(".", 1)[-1].lower() \
                                if "." in oid.rsplit("/", 1)[-1] else ""
                            if suffix not in TEXT_EXTENSIONS:
                                continue
                            try:
                                with open(onode.path, "r", encoding="utf-8",
                                          errors="ignore") as fh:
                                    txt = fh.read(2 * 1024 * 1024)
                            except OSError:
                                continue
                            fp = _hashing.simhash(txt)
                            if fp:
                                raw_fps[oid] = fp
                                budget -= 1
                    for key, fp in sorted(raw_fps.items()):
                        oid = key if key in {o for o, _c in others} \
                            else hash_to_id.get(key, "")
                        if not oid or oid == pseudo.id or not fp:
                            continue
                        dist = _hashing.hamming_distance(fp_new, fp)
                        if dist <= 3:
                            near_duplicates.append({"id": oid, "hamming": dist})
                    near_duplicates.sort(key=lambda d: (d["hamming"], d["id"]))
                    near_duplicates = near_duplicates[:10]
            else:
                exact_duplicate_of = ""

            return {
                "path": pseudo.id,
                "unchanged": unchanged,
                "new_hash": new_hash,
                "current_hash": current_hash,
                "refs_gained": sorted(resolved - current_targets),
                "refs_lost": sorted(current_targets - resolved),
                "dangling_now": dangling,
                "exact_duplicate_of": exact_duplicate_of,
                "near_duplicates": near_duplicates,
                "dependents_count": dependents_count,
            }

    def _op_impact(self, params: dict) -> dict:
        with self._crawl_lock:
            return self._federated_impact(params.get("path", ""))

    def _op_context(self, params: dict) -> dict:
        with self._crawl_lock:
            path = params.get("path", "")
            node_id, store = self._federated_resolve(path)
            if node_id is None:
                return {"error": f"unknown path: {path}"}
            api = QueryAPI(store)
            ctx = api.context_for(node_id)
            # Tag cross-dir links in the context.
            for link in ctx.get("links", []):
                node = store.get_node(link["id"])
                if node and node.attrs.get("external"):
                    link["cross_dir"] = True
                    link["dir"] = node.attrs.get("target_dir") or node.attrs.get("source_dir", "")
            return ctx

    def _op_neighbors(self, params: dict) -> dict:
        with self._crawl_lock:
            path = params.get("path", "")
            node_id, store = self._federated_resolve(path)
            if node_id is None:
                return {"error": f"unknown path: {path}", "neighbors": []}
            api = QueryAPI(store)
            types = params.get("types")
            if types:
                types = [EdgeType(t) for t in types]
            return api.neighbors(node_id, edge_types=types, depth=int(params.get("depth", 1)))

    def _op_search(self, params: dict) -> dict:
        # Search across every known store (multi-root), merging results.
        text = params.get("text", "")
        limit = int(params.get("limit", 50))
        with self._crawl_lock, self._store_lock:
            stores = list(self._stores.values())
        # Dedupe stores (a merged parent + absorbed subdir share one store obj).
        seen_stores: set[int] = set()
        hits: list[dict] = []
        seen_keys: set[tuple[str, str]] = set()  # (fragment_root, node_id)
        for store in stores:
            if id(store) in seen_stores:
                continue
            seen_stores.add(id(store))
            root = getattr(store, "root", "")
            for hit in QueryAPI(store).search(text, limit=limit):
                key = (root, str(hit.get("id", "")))
                if key in seen_keys:
                    continue
                seen_keys.add(key)
                hits.append(hit)
        # Global deterministic order across fragments (each store's own results
        # are already id-sorted; merging per-store would bias early stores).
        hits.sort(key=lambda h: h["id"])
        return {"results": hits[:limit]}

    def _op_summary(self, params: dict) -> dict:
        with self._crawl_lock:
            root = params.get("root", "")
            api = self._api_for(root) if root else (self.api or self._api_for(""))
            summary = api.summary()
            # Federation view: aggregate every fragment store, minus the shadow
            # (external=true) nodes cross-linking mirrored in from other dirs,
            # plus a per-fragment breakdown so the dashboard can show each root.
            with self._store_lock:
                store_items = list(self._stores.items())
            seen: set[int] = set()
            shadows = 0
            counts_by_root: dict[str, dict[str, int]] = {}
            for frag_root, store in store_items:
                if id(store) in seen:
                    continue
                seen.add(id(store))
                shadows += sum(1 for n in store.all_nodes()
                               if n.attrs.get("external"))
                counts_by_root[frag_root] = store.counts()
            total = self._aggregate_counts()
            total["nodes"] = max(0, total["nodes"] - shadows)
            summary["total"] = total
            summary["shadows"] = shadows
            summary["fragments"] = [
                {"root": r, "nodes": counts_by_root[r]["nodes"],
                 "edges": counts_by_root[r]["edges"]}
                for r in sorted(counts_by_root)
            ]
            return summary

    def _op_roots(self, params: dict) -> dict:
        """List every crawl root known to this daemon (multi-root registry)."""
        with self._store_lock:
            return {"roots": sorted(self._stores.keys()),
                    "active": self.store.root}

    def _op_graph(self, params: dict) -> dict:
        """Cold-start snapshot for the dashboard canvas.

        The SSE ring only carries events emitted since the daemon started, so a
        restarted daemon serves a live stream over an empty canvas even though
        graph.db is fine. This op replays the persisted graph instead. Nodes
        come from every fragment; edges are prioritised contains → references →
        duplicate_of → similar_to and capped (``max_edges``, default 30000)
        because similarity dimensions can carry hundreds of thousands of edges
        that are useless at canvas scale.
        """
        max_edges = int(params.get("max_edges", 30000))
        with self._crawl_lock:
            with self._store_lock:
                store_items = list(self._stores.items())
            nodes: list[dict] = []
            seen_nodes: set[str] = set()
            by_type: dict[str, list[dict]] = {}
            seen: set[int] = set()
            for frag_root, store in store_items:
                if id(store) in seen:
                    continue
                seen.add(id(store))
                for n in store.all_nodes():
                    if n.id in seen_nodes:
                        continue
                    seen_nodes.add(n.id)
                    kind = n.kind.value if hasattr(n.kind, "value") else str(n.kind)
                    nodes.append({"id": n.id, "node_kind": kind})
                for e in store.edges(None):
                    by_type.setdefault(e.type.value, []).append(
                        {"src": e.src, "dst": e.dst, "edge_type": e.type.value}
                    )
            ordered: list[dict] = []
            for t in ("contains", "references", "duplicate_of", "similar_to"):
                if len(ordered) >= max_edges:
                    break
                ordered.extend(by_type.get(t, [])[: max_edges - len(ordered)])
            active = self.store.root if self.store else ""
            return {
                "root": active,
                "fragments": sorted(self._stores.keys()),
                "nodes": nodes,
                "edges": ordered,
                "edges_truncated": sum(len(v) for v in by_type.values()) - len(ordered),
            }

    def _api_for(self, path: str) -> QueryAPI:
        """Pick the QueryAPI for the store that owns ``path`` (multi-root).

        Resolution order:
          1. If ``path`` looks like an absolute path under a known root, use
             that root's store.
          2. Else search every known store for a node matching ``path`` (the
             QueryAPI.to_id does suffix matching, so a bare ``"a.py"`` finds
             the right node regardless of which root it lives under).
          3. Else fall back to the active store (``self.api``) — preserving the
             single-root behaviour the API/tests expect.
        """
        if path:
            store = self._store_for(path)
            if store is not None:
                if store is self.store and self.api is not None:
                    return self.api
                return QueryAPI(store)
            # Bare id / relative path: find the store that actually has it.
            with self._store_lock:
                stores = list(self._stores.values())
            seen: set[int] = set()
            for store in stores:
                if id(store) in seen:
                    continue
                seen.add(id(store))
                api = QueryAPI(store)
                if api.to_id(path) is not None:
                    return api
        if self.api is not None:
            return self.api
        return QueryAPI(self.store)

    def _federated_resolve(self, path: str) -> tuple[str | None, GraphStore | None]:
        """Resolve a user path to (node_id, store) across all fragments.

        Tries each fragment's ``to_id``; also tries splitting on the first path
        separator so ``utils/helper.py`` resolves in the ``utils`` fragment
        (where the id is ``helper.py``). Returns (None, None) if unresolved.
        """
        if not path:
            return None, None
        with self._store_lock:
            stores = list(self._stores.values())
        seen: set[int] = set()
        for store in stores:
            if id(store) in seen:
                continue
            seen.add(id(store))
            api = QueryAPI(store)
            node_id = api.to_id(path)
            if node_id is not None:
                return node_id, store
            # Try splitting: "utils/helper.py" -> look in the utils fragment
            # for "helper.py". Match the subdir by checking if any fragment
            # root ends with the first path segment.
            parts = path.replace("\\", "/").split("/", 1)
            if len(parts) == 2:
                head, tail = parts
                for s2 in stores:
                    if s2.root.replace("\\", "/").endswith("/" + head) or \
                       Path(s2.root).name == head:
                        api2 = QueryAPI(s2)
                        nid = api2.to_id(tail)
                        if nid is not None:
                            return nid, s2
        return None, None

    def _federated_impact(self, path: str) -> dict:
        """Blast radius across all fragments, hopping stores via shadow nodes.

        Fast path: if the node has no cross-dir shadow dependents, delegate to
        the Rust ``impact_of`` (one call, O(reachable)). Slow path: only when
        cross-dir shadow nodes are hit does the Python BFS hop stores.
        """
        node_id, store = self._federated_resolve(path)
        if node_id is None:
            return {"error": f"unknown path: {path}", "direct": [], "transitive": []}

        # Fast path: use the store's native impact_of (Rust, O(reachable)).
        api = QueryAPI(store)
        base = api.impact_of(node_id)
        # Check whether any direct dependent is a cross-dir shadow node. If not,
        # the blast radius is entirely within this fragment — return as-is.
        has_cross = False
        for entry in base.get("direct", []) + base.get("transitive", []):
            eid = entry["id"] if isinstance(entry, dict) else entry
            node = store.get_node(eid)
            if node and node.attrs.get("external"):
                has_cross = True
                break
        if not has_cross:
            # Convert to rich entries (dicts with id) for a consistent API.
            direct = [{"id": d["id"] if isinstance(d, dict) else d} for d in base.get("direct", [])]
            transitive = [{"id": t["id"] if isinstance(t, dict) else t} for t in base.get("transitive", [])]
            return {"target": base["target"], "direct": direct, "transitive": transitive,
                    "total_affected": base.get("total_affected", len(direct) + len(transitive)),
                    "truncated": base.get("truncated", False)}

        # Slow path: cross-dir hops needed — run the federated BFS.
        return self._federated_impact_bfs(node_id, store)

    def _federated_impact_bfs(self, node_id: str, store) -> dict:
        """The cross-store BFS (only used when cross-dir shadow nodes are hit).

        ``seen`` is keyed by ``(fragment_root, node_id)``: node ids are
        root-relative *per fragment*, so the bare id alone can collide across
        fragments and silently drop dependents. Shadow (external) hits are
        reported under the real node's id in its owning fragment's namespace
        whenever it can be resolved (falling back to the absolute-path shadow
        id), with ``cross_dir``/``dir`` marking the hop.
        """
        from collections import deque

        def _root_of(s) -> str:
            return getattr(s, "root", "")

        direct: list[dict] = []
        transitive: list[dict] = []
        seen: set[tuple[str, str]] = {(_root_of(store), node_id)}
        frontier: deque[tuple[str, object, int]] = deque([(node_id, store, 0)])
        # Lazily-built path -> real-node-id index per fragment store, so
        # resolving a shadow node's counterpart is one dict lookup instead of
        # an O(N) scan per hit.
        path_index_cache: dict[int, dict[str, str]] = {}

        def _path_index(st) -> dict[str, str]:
            key = id(st)
            idx = path_index_cache.get(key)
            if idx is None:
                idx = {
                    n.path: n.id for n in st.all_nodes()
                    if n.kind == NodeKind.FILE and not n.attrs.get("external")
                }
                path_index_cache[key] = idx
            return idx

        while frontier:
            current, cur_store, depth = frontier.popleft()
            cur_key = _root_of(cur_store)
            for edge in cur_store.in_edges(current, EdgeType.REFERENCES):
                src = edge.src
                if (cur_key, src) in seen:
                    continue
                seen.add((cur_key, src))

                hops: list[tuple[str, object]] = []  # (next_id, next_store)
                src_node = cur_store.get_node(src)
                if src_node and src_node.attrs.get("external"):
                    other_dir = src_node.attrs.get("source_dir") or src_node.attrs.get("target_dir")
                    other_store = self._store_for(other_dir) if other_dir else None
                    if other_store is None and other_dir:
                        other_db = Path(other_dir) / ".dataworm" / "graph.db"
                        if other_db.exists():
                            try:
                                other_store = load_sqlite(other_db)
                                other_store.bus = self.bus
                                with self._store_lock:
                                    self._stores[other_dir] = other_store
                            except Exception:
                                other_store = None
                    entry: dict = {"cross_dir": True, "dir": other_dir or ""}
                    real_id = (
                        _path_index(other_store).get(src_node.path)
                        if other_store is not None else None
                    )
                    if real_id and not (other_store is store and real_id == node_id):
                        # Report + traverse the real node in its own namespace.
                        okey = _root_of(other_store)
                        entry["id"] = real_id
                        if (okey, real_id) not in seen:
                            seen.add((okey, real_id))
                            hops.append((real_id, other_store))
                    else:
                        # Unresolvable shadow — report the absolute path.
                        entry["id"] = src
                else:
                    entry = {"id": src}
                    hops.append((src, cur_store))

                if depth == 0:
                    direct.append(entry)
                else:
                    transitive.append(entry)
                for next_id, next_store in hops:
                    frontier.append((next_id, next_store, depth + 1))

        direct.sort(key=lambda e: e["id"])
        transitive.sort(key=lambda e: e["id"])
        IMPACT_CAP = 1000
        truncated = len(direct) > IMPACT_CAP or len(transitive) > IMPACT_CAP
        direct = direct[:IMPACT_CAP]
        transitive = transitive[:IMPACT_CAP]
        return {
            "target": node_id,
            "direct": direct,
            "transitive": transitive,
            "total_affected": len(direct) + len(transitive),
            "truncated": truncated,
        }

    def _op_signature(self, params: dict) -> dict:
        """Graph signature. Rust path if available (it's a hot op on large graphs)."""
        if self.rust is not None:
            # Rust EdgeData uses "edge_type"; Node.to_dict/Edge.to_dict use "type".
            nodes = [n.to_dict() for n in self.store.all_nodes()]
            edges = [
                {"src": e.src, "dst": e.dst, "edge_type": e.type.value,
                 "weight": e.weight, "attrs": e.attrs}
                for e in self.store.all_edges()
            ]
            return self.rust.dispatch("signature", {"nodes": nodes, "edges": edges})
        return {"hash_hex": self.store.signature()}

    def _op_hash_pass(self, params: dict) -> dict:
        """Recompute duplicate_of edges. Rust path if available."""
        if self.rust is not None:
            # Rust expects "edge_type" not "type" (Edge.to_dict naming).
            snap = {
                "root": self.store.root,
                "nodes": [n.to_dict() for n in self.store.all_nodes()],
                "edges": [
                    {"src": e.src, "dst": e.dst, "edge_type": e.type.value,
                     "weight": e.weight, "attrs": e.attrs}
                    for e in self.store.all_edges()
                ],
                "meta": dict(self.store.meta),
            }
            edges = self.rust.dispatch("hash_pass", snap)
            return {"edges": edges}
        # Python fallback
        from dataworm.engine import hashing_pass
        from dataworm.config import Config
        hashing_pass(self.store, Config(root=self.store.root))
        return {"edges": [e.to_dict() for e in self.store.edges(EdgeType.DUPLICATE_OF)]}

    def _op_extract_refs(self, params: dict) -> dict:
        """Extract raw references from a file. Rust path if available."""
        path = params.get("path", "")
        if self.rust is not None:
            return self.rust.dispatch("extract_refs", {"path": path})
        # Python fallback
        from dataworm.extractors.references import extract_raw_references
        from dataworm.models import Node, NodeKind
        try:
            text = Path(path).read_text(encoding="utf-8", errors="ignore")
        except OSError:
            return {"refs": []}
        node = Node(id=Path(path).name, path=path, kind=NodeKind.FILE)
        return {"refs": extract_raw_references(node, text)}

    def _op_shutdown(self, params: dict) -> dict:
        """Flush + persist, signalling the daemon to exit."""
        # Stop all watchers so the daemon can exit cleanly.
        self.stop_watchers()
        self._save()
        return {"ok": True, "status": "shutting down"}

    # ---- filesystem watching -------------------------------------------

    def _op_watch(self, params: dict) -> dict:
        """Start watching a root for changes (idempotent per root).

        On the first fs_event under ``root``, a debounced incremental re-crawl
        is scheduled (see ``_schedule_recrawl``). The re-crawl re-runs the
        reference/hashing/semantic passes so new/changed links materialize and
        the dashboard re-ranks live.
        """
        from dataworm.watcher import DirectoryWatcher
        root = params.get("root", "")
        if not root:
            return {"error": "watch requires 'root'"}
        root = str(Path(root).resolve())
        if root in self._watchers:
            return {"ok": True, "status": "already watching", "root": root,
                    "backend": self._watchers[root].backend}
        poll_interval = float(params.get("poll_interval", 1.5))
        if "webhook_url" in params:
            # Convenience: arm the Reflex Arc webhook outbox along with the watch.
            self.webhook_url = str(params.get("webhook_url") or "").strip() or None
        config = Config(root=root)
        watcher = DirectoryWatcher(
            root=root, bus=self.bus, config=config,
            poll_interval=poll_interval, on_event=self._on_fs_event,
        )
        watcher.start()
        self._watchers[root] = watcher
        self.bus.emit("fs_watch_started", root=root, backend=watcher.backend)
        return {"ok": True, "status": "watching", "root": root, "backend": watcher.backend}

    def _op_unwatch(self, params: dict) -> dict:
        root = params.get("root", "")
        if not root:
            return {"error": "unwatch requires 'root'"}
        root = str(Path(root).resolve())
        watcher = self._watchers.pop(root, None)
        if watcher is None:
            return {"ok": True, "status": "not watching", "root": root}
        watcher.stop()
        self.bus.emit("fs_watch_stopped", root=root)
        return {"ok": True, "status": "stopped watching", "root": root}

    def _op_watched(self, params: dict) -> dict:
        return {"roots": list(self._watchers.keys()),
                "backends": {r: w.backend for r, w in self._watchers.items()}}

    # ---- Reflex Arc ops --------------------------------------------------

    def _op_changes(self, params: dict) -> dict:
        """Replay journalled change reports (the worm's memory of changes).

        Fans out over every fragment database's ``journal`` table via
        short-lived sqlite3 connections, merges the reports and returns
        ``{"changes": [...], "last_seq": <max seq>}``.

        Params: ``since_seq`` (only reports with a higher journal seq),
        ``root`` (restrict to one fragment), ``limit`` (default 200).

        Per-fragment ``seq`` counters are independent, so the global order is
        by ``ts`` (ties: root, seq) and ``since_seq`` paging is per-fragment
        approximate across fragments — good enough for replay-style consumers.
        """
        since_seq = int(params.get("since_seq", 0))
        limit = max(1, int(params.get("limit", 200)))
        root_filter = str(params.get("root") or "")
        with self._store_lock:
            roots = sorted(self._stores.keys())
        if root_filter:
            want = os.path.realpath(root_filter)
            roots = [r for r in roots if os.path.realpath(r) == want]
        merged: list[dict] = []
        seen: set[tuple[str, int]] = set()
        for frag_root in roots:
            db_path = Path(frag_root) / ".dataworm" / "graph.db"
            if not db_path.exists():
                continue
            try:
                con = sqlite3.connect(str(db_path))
                try:
                    ensure_journal(con)
                    rows = fetch_since(con, since_seq, limit=limit)
                finally:
                    con.close()
            except Exception as exc:
                log.warning("changes: failed to read journal %s: %s",
                            db_path, exc)
                continue
            for report in rows:
                key = (str(report.get("root", frag_root)),
                       int(report.get("seq", 0)))
                if key in seen:
                    continue  # same physical row via two registered roots
                seen.add(key)
                merged.append(report)
        merged.sort(key=lambda r: (float(r.get("ts", 0.0)),
                                   str(r.get("root", "")),
                                   int(r.get("seq", 0))))
        merged = merged[:limit]
        last_seq = max((int(r.get("seq", 0)) for r in merged), default=0)
        return {"changes": merged, "last_seq": last_seq}

    def _op_configure_webhook(self, params: dict) -> dict:
        """Point the change-report webhook outbox at ``url``.

        An empty string disables delivery. Delivery is best-effort v1: after
        each incremental recrawl that appended journal reports, every
        un-notified report of the touched fragment databases is POSTed (JSON,
        5s timeout); 2xx marks it notified, failures stay un-notified and are
        retried on the next trigger.
        """
        url = str(params.get("url", "") or "").strip()
        self.webhook_url = url or None
        return {"ok": True, "webhook_url": self.webhook_url or ""}

    # ---- Reflex Arc helpers ---------------------------------------------

    def _snapshot_node_state(self, store, node_id: str) -> dict:
        """Hash + resolved REFERENCES neighbourhood of one node.

        Returns ``{"hash", "refs", "dependents"}``; empty when the node does
        not exist (also guards stores whose edge iterators require live ids).
        Used for the pre/post delta diff in the incremental re-crawl.
        """
        node = store.get_node(node_id) if node_id else None
        if node is None:
            return {"hash": "", "refs": [], "dependents": []}
        return {
            "hash": node.content_hash or "",
            "refs": sorted(e.dst for e in store.out_edges(node_id, EdgeType.REFERENCES)),
            "dependents": sorted(e.src for e in store.in_edges(node_id, EdgeType.REFERENCES)),
        }

    def _flush_webhook(self, touched_dbs: set[str]) -> None:
        """Drain the webhook outbox for the given fragment databases.

        Best-effort v1 (documented behaviour): POST every un-notified journal
        report (json body, 5s timeout); 2xx responses mark the row notified,
        anything else stays un-notified and is retried on the next recrawl
        that touches the same fragment db. Never raises into the re-crawl
        path; failures are logged.
        """
        url = self.webhook_url
        if not url or not touched_dbs:
            return
        for db_path in sorted(touched_dbs):
            try:
                con = sqlite3.connect(db_path)
                try:
                    ensure_journal(con)
                    rows = con.execute(
                        "SELECT seq, report_json FROM journal"
                        " WHERE notified = 0 ORDER BY seq ASC").fetchall()
                finally:
                    con.close()
            except Exception as exc:
                log.warning("webhook outbox read failed for %s: %s",
                            db_path, exc)
                continue
            delivered: list[int] = []
            for seq, raw in rows:
                try:
                    report = json.loads(raw)
                except (TypeError, ValueError):
                    delivered.append(seq)  # poison row: mark to avoid a loop
                    continue
                if self._post_webhook(url, report):
                    delivered.append(seq)
            if delivered:
                try:
                    con = sqlite3.connect(db_path)
                    try:
                        mark_notified(con, delivered)
                    finally:
                        con.close()
                except Exception as exc:
                    log.warning("webhook outbox mark failed for %s: %s",
                                db_path, exc)

    def _post_webhook(self, url: str, report: dict) -> bool:
        """POST one report as JSON. True only on a 2xx response."""
        try:
            data = json.dumps(report).encode("utf-8")
            req = urllib.request.Request(
                url, data=data,
                headers={"Content-Type": "application/json"}, method="POST")
            with urllib.request.urlopen(req, timeout=5) as resp:
                status = int(getattr(resp, "status", 200) or 200)
                return 200 <= status < 300
        except Exception as exc:
            log.warning("webhook POST to %s failed: %s", url, exc)
            return False

    def stop_watchers(self) -> None:
        """Stop every active watcher. Called on daemon shutdown."""
        for watcher in list(self._watchers.values()):
            try:
                watcher.stop()
            except Exception:
                log.exception("watcher stop failed")
        self._watchers.clear()

    def _on_fs_event(self, kind: str, path: str) -> None:
        """Called by every DirectoryWatcher (direct callback, in addition to bus)."""
        # Snapshot the changed path + its raw event kind; the scheduler
        # coalesces bursts into one crawl (one report per changed path).
        self._recrawl_lock.acquire()
        try:
            self._changed_paths.add(path)
            self._changed_kinds.setdefault(path, set()).add(kind)
        finally:
            self._recrawl_lock.release()
        self._schedule_recrawl()

    def _schedule_recrawl(self) -> None:
        """Debounce bursts of fs events into a single incremental re-crawl.

        If a re-crawl is already pending, just record more changed paths and
        return; the running worker will pick them up on its next cycle. If none
        is running, start one after ``debounce`` seconds of quiet.
        """
        debounce = 0.6
        if self._recrawl_thread is not None and self._recrawl_thread.is_alive():
            # A crawl is running; signal it to do another pass for the new changes.
            self._recrawl_pending.set()
            return
        def _worker():
            while True:
                # Wait for a quiet period, then run.
                self._recrawl_pending.wait(debounce)
                self._recrawl_pending.clear()
                try:
                    self._recrawl_incremental()
                except Exception:
                    log.exception("incremental re-crawl failed")
                # If more events arrived during the crawl, loop; else exit.
                if not self._recrawl_pending.is_set():
                    break
                self._recrawl_pending.wait(debounce)
                if not self._recrawl_pending.is_set():
                    break
        self._recrawl_thread = threading.Thread(target=_worker, daemon=True)
        self._recrawl_thread.start()

    def _recrawl_incremental(self) -> None:
        """Re-crawl only the fragment(s) containing changed files (federated).

        Finds which fragment store owns each changed path, re-crawls just those
        fragments (not the whole tree), re-runs convergence + cross-linking on
        them, saves only those fragments to their own ``<dir>/.dataworm/graph.db``,
        and emits a ``cross_dir_impact`` event if any cross-links were affected.
        """
        from dataworm.crawler import crawl, crawl_shallow
        from dataworm.engine import reference_pass, semantic_pass, hashing_pass

        # Collect the changed paths (+ raw fs event kinds, for the Reflex Arc
        # report labels) and map them to their owning fragments.
        with self._recrawl_lock:
            changed = list(self._changed_paths)
            self._changed_paths.clear()
            kinds_raw = {p: set(k) for p, k in self._changed_kinds.items()}
            self._changed_kinds.clear()
        if not changed:
            return

        # Find which fragment roots own the changed paths.
        affected_roots: set[str] = set()
        paths_by_frag: dict[str, list[str]] = {}
        with self._store_lock:
            stores = list(self._stores.items())
        # Canonicalize fragment roots once (resolve short-name / symlink forms).
        canon_roots = [(os.path.realpath(frag_root), frag_root, _store)
                       for frag_root, _store in stores]
        for ch in changed:
            ch_real = os.path.realpath(ch)
            # Pick the DEEPEST (most specific) fragment root containing the path.
            best_root = None
            best_len = 0
            for canon_root, frag_root, _store in canon_roots:
                try:
                    Path(ch_real).relative_to(canon_root)
                    if len(canon_root) > best_len:
                        best_root = frag_root
                        best_len = len(canon_root)
                except ValueError:
                    continue
            if best_root:
                affected_roots.add(best_root)
                paths_by_frag.setdefault(best_root, []).append(ch_real)
        if not affected_roots:
            return

        # Collapse each path's raw fs event kinds into one Reflex Arc report
        # kind (a burst often delivers created+modified for the same path).
        # Priority: the terminal state wins — deleted > moved > created >
        # modified — because that is what the post-crawl graph reflects.
        kinds_by_path: dict[str, str] = {}
        for ch in changed:
            ch_real = os.path.realpath(ch)
            raw_kinds = kinds_raw.get(ch, set())
            for fs_kind in ("fs_deleted", "fs_moved", "fs_created",
                            "fs_modified"):
                if fs_kind in raw_kinds:
                    kinds_by_path[ch_real] = fs_kind[len("fs_"):]
                    break
            else:
                kinds_by_path.setdefault(ch_real, "modified")

        self.bus.emit("start", root=",".join(sorted(affected_roots)),
                      max_cycles=1, reason="fs_event")

        # Per-fragment Reflex Arc capture state: changed abs path -> rel node
        # id, and rel node id -> pre-recrawl snapshot (hash/refs/dependents).
        rel_by_frag: dict[str, dict[str, str]] = {}
        pre_by_frag: dict[str, dict[str, dict]] = {}

        # Re-crawl each affected fragment + re-run convergence + cross-linking.
        # The whole mutation phase runs under the SAME crawl lock manual
        # crawls hold (_op_crawl): read handlers take that lock too, so a
        # dashboard query concurrent with an fs-event re-crawl sees either the
        # pre- or post-recrawl graph — never a fragment mid clear/rebuild
        # (e.g. with its CONTAINS edges cleared). RLock, so a future caller
        # that already holds it can re-enter safely instead of deadlocking.
        mutated_by_linking: set[str] = set()
        with self._crawl_lock:
            for frag_root in affected_roots:
                with self._store_lock:
                    frag_store = self._stores.get(frag_root)
                if frag_store is None:
                    continue
                # Semantic + hashing are ON for the incremental re-crawl now that
                # pass outputs are memoized by content_hash: unchanged files cost
                # no reads/embeds, so these dimensions no longer go stale after
                # live edits (the old enable_*=False left them silently outdated).
                config = Config(root=frag_root, enable_semantic=True, enable_hashing=True)
                # --- Reflex Arc: snapshot the PRE state of every changed path ---
                # (hash, resolved outgoing refs, incoming dependents) BEFORE the
                # re-crawl rebuilds nodes/edges — reference_pass clears and
                # recreates all REFERENCES edges, so this is the only chance to
                # capture the "before" side of the delta.
                frag_canon = os.path.realpath(frag_root)
                rel_by_frag[frag_root] = {}
                pre_by_frag[frag_root] = {}
                for ch_real in paths_by_frag.get(frag_root, []):
                    try:
                        rel = Path(ch_real).relative_to(frag_canon).as_posix()
                    except ValueError:
                        continue  # not under this fragment after canonicalisation
                    rel_by_frag[frag_root][ch_real] = rel
                    pre_by_frag[frag_root][rel] = self._snapshot_node_state(
                        frag_store, rel)
                # Re-walk: shallow for the root fragment, full for subdirs.
                root_frag = frag_root == self.store.root
                used_rust = False
                if self.rust is not None:
                    try:
                        # Rust crawl + snapshot apply handles clearing contains edges
                        # and dropping stale nodes (incremental via mtime reuse).
                        self._rust_crawl(frag_store, frag_root, config, shallow=root_frag)
                        used_rust = True
                    except Exception as exc:
                        log.warning("rust re-crawl of %s failed (%s); python fallback",
                                    frag_root, exc)
                if not used_rust:
                    # Re-crawl the fragment (incremental by mtime).
                    frag_store.clear_edges(EdgeType.CONTAINS)
                    # Remove stale nodes (files that vanished) in one batch.
                    stale_ids = [
                        node.id for node in frag_store.all_nodes()
                        if node.id and not node.attrs.get("external")
                        and not Path(node.path).exists()
                    ]
                    if stale_ids:
                        frag_store.remove_nodes_batch(stale_ids)
                    if root_frag:
                        crawl_shallow(frag_store, config)
                    else:
                        crawl(frag_store, config)
                # Re-run convergence on this fragment.
                if self.rust is not None and _store_is_rust(frag_store):
                    # Seed/harvest the native memo maps around the convergence
                    # loop (same rules as the Python fallback branch).
                    frag_store._rust_memos_push()
                    result = json.loads(frag_store._inner.run_convergence(
                        1, config.max_content_bytes, sorted(config.text_extensions),
                        config.max_semantic_nodes, config.similarity_threshold,
                        False, config.enable_hashing, config.max_hashing_nodes,
                    ))
                    frag_store._rust_memos_pull()
                    # Replay events (live dashboard) + capture dangling for cross-links.
                    self._frag_dangling[frag_root] = self._replay_convergence_events(
                        result.get("events", []))
                else:
                    frag_dangling: dict[str, list[str]] = {}
                    for fn in (reference_pass, hashing_pass, semantic_pass):
                        res = fn(frag_store, config)
                        if fn is reference_pass and isinstance(res, dict):
                            frag_dangling = res
                    self._frag_dangling[frag_root] = frag_dangling
                # Save only this fragment.
                db_path = Path(frag_root) / ".dataworm" / "graph.db"
                try:
                    save_sqlite(frag_store, db_path)
                except Exception as exc:
                    log.warning("failed to save %s: %s", db_path, exc)

            # Re-link cross-dir refs across ALL fragments + check cross-impact.
            # Cross-linking mutates other fragments' stores too (shadow nodes +
            # mirrored edges), so it collects which roots it touched; they are
            # persisted below (an affected fragment may gain shadow nodes only
            # AFTER its own save above).
            with self._store_lock:
                all_stores = list(self._stores.items())
            cross_links, mutated_by_linking = self._link_cross_dir_refs(
                all_stores, Config(root=self.store.root))

            # Persist every fragment whose store cross-linking actually touched
            # (both the source and the mirrored target). Best-effort per
            # fragment: one unwritable db must not abort the others.
            store_by_root = dict(all_stores)
            for link_root in sorted(mutated_by_linking):
                link_store = store_by_root.get(link_root)
                if link_store is None:
                    continue
                link_db = Path(link_root) / ".dataworm" / "graph.db"
                try:
                    save_sqlite(link_store, link_db)
                except Exception as exc:
                    log.warning("failed to save %s: %s", link_db, exc)

        # --- Reflex Arc: assemble + journal + broadcast per-change reports ---
        # Runs AFTER convergence and cross-linking so dependents/refs reflect
        # the fully-settled graph (cross-dir shadow links included).
        reports_to_flush: list[tuple[str, dict]] = []  # (fragment db path, report)
        for frag_root in sorted(paths_by_frag):
            rel_ids = rel_by_frag.get(frag_root) or {}
            if not rel_ids:
                continue
            with self._store_lock:
                frag_store = self._stores.get(frag_root)
            if frag_store is None:
                continue
            db_path = str(Path(frag_root) / ".dataworm" / "graph.db")
            dang = self._frag_dangling.get(frag_root, {})
            pre_states = pre_by_frag.get(frag_root, {})
            frag_reports, burst = self._diff_fragment_reports(
                frag_store, frag_root, pre_states,
                source="fs_event",
                kinds_override={rel: kinds_by_path.get(ch_real, "modified")
                                for ch_real, rel in rel_ids.items()},
                dang=dang,
            )
            for report in frag_reports:
                reports_to_flush.append((db_path, report))
            if burst is not None:
                reports_to_flush.append((db_path, burst))

        self._publish_reports(reports_to_flush)

        # Emit cross_dir_impact if cross-links were (re)established and an
        # affected fragment is involved. A cross-link touching an affected
        # fragment means a local change has cross-dir blast radius.
        affected_impact: list[dict] = []
        if cross_links:
            for frag_root in affected_roots:
                frag_store = self._stores.get(frag_root)
                if frag_store is None:
                    continue
                # Check both directions: outgoing (target_dir) and incoming (source_dir).
                cross_edges = [e for e in frag_store.edges(EdgeType.REFERENCES)
                               if e.attrs.get("cross_dir")]
                if cross_edges:
                    other_dirs = {e.attrs.get("source_dir") or e.attrs.get("target_dir", "")
                                  for e in cross_edges}
                    other_dirs.discard("")
                    affected_impact.append({
                        "changed_dir": frag_root,
                        "affected_dirs": sorted(other_dirs),
                    })
        if affected_impact:
            self.bus.emit("cross_dir_impact", fragments=affected_impact)

        counts = self._aggregate_counts()
        self.bus.emit("done", converged=True, cycles=1, counts=counts,
                      root=",".join(sorted(affected_roots)), reason="fs_event")


# ---- method registry -------------------------------------------------------

_METHODS: dict[str, Any] = {
    "ping": Core._op_ping,
    "crawl": Core._op_crawl,
    "impact": Core._op_impact,
    "context": Core._op_context,
    "neighbors": Core._op_neighbors,
    "search": Core._op_search,
    "summary": Core._op_summary,
    "signature": Core._op_signature,
    "hash_pass": Core._op_hash_pass,
    "extract_refs": Core._op_extract_refs,
    "watch": Core._op_watch,
    "unwatch": Core._op_unwatch,
    "watched": Core._op_watched,
    "changes": Core._op_changes,
    "plan_edit": Core._op_plan_edit,
    "configure_webhook": Core._op_configure_webhook,
    "roots": Core._op_roots,
    "graph": Core._op_graph,
    "shutdown": Core._op_shutdown,
}
