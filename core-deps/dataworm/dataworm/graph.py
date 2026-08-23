"""GraphStore: the worm's in-memory graph.

The graph lives in **Rust** memory (`RustGraphStore` via PyO3) when the
``dataworm._rust`` extension is available; Python holds a thin handle and
crosses the boundary once per mutation/query. The whole graph never
materialises in Python. Bus-event emission stays on the Python side: each
mutating method calls the Rust store, then emits ``node`` / ``edge`` /
``reset_dim`` on the bus so the live dashboard keeps animating.

When the Rust extension is unavailable (``--no-rust`` or a build mismatch),
``PythonGraphStore`` (a pure-Python networkx implementation) is used instead.
Both backends expose the identical Python API below and must produce identical
``counts()`` / ``signature()`` / ``all_nodes()`` / ``all_edges()`` results — this
is the parity guarantee the test suite checks.

Public API (stable, backend-agnostic):
  add_node(Node) / has_node / get_node -> Node | None
  add_edge(Edge) / get_edge -> Edge | None
  nodes(kind) / node_ids / all_nodes -> list[Node]
  edges(type) / out_edges / in_edges / all_edges -> Iterator[Edge]
  clear_edges(type) / remove_node / counts / signature
  attach_root / merge(GraphStore) / roots
"""

from __future__ import annotations

import json
from pathlib import Path, PurePosixPath
from typing import Any, Iterable, Iterator

from dataworm.events import EventBus
from dataworm.models import Edge, EdgeType, Node


def _try_rust():
    """Return the dataworm._rust module, or None if unavailable."""
    try:
        import dataworm._rust as rust  # type: ignore
        if hasattr(rust, "RustGraphStore"):
            return rust
    except Exception:
        pass
    return None


# ---- pure-Python fallback (networkx) -------------------------------------
# Kept for --no-rust and the parity tests. Must match the Rust store exactly.

class PythonGraphStore:
    """Networkx-backed GraphStore. The fallback when Rust is unavailable."""

    def __init__(self, root: str = "", bus: EventBus | None = None) -> None:
        self.root = root
        self.roots: set[str] = {root} if root else set()
        import networkx as nx
        self.g: nx.MultiDiGraph = nx.MultiDiGraph()
        self.meta: dict[str, Any] = {}
        self.bus = bus
        # Content-addressed memo: pass outputs keyed by sha256 content_hash so
        # repeated cycles/recrawls skip re-extraction/re-embedding of unchanged
        # files. Persisted with the graph (persist.save_sqlite/load_sqlite).
        self.memo: dict[str, dict] = {"refs": {}, "simhash": {}, "embed": {}}

    def add_node(self, node: Node) -> None:
        is_new = not self.g.has_node(node.id)
        self.g.add_node(node.id, data=node)
        if is_new and self.bus is not None:
            self.bus.emit(
                "node", id=node.id, node_kind=node.kind.value,
                path=node.path, size=node.size,
            )

    def has_node(self, node_id: str) -> bool:
        return self.g.has_node(node_id)

    def get_node(self, node_id: str) -> Node | None:
        data = self.g.nodes[node_id].get("data") if self.g.has_node(node_id) else None
        return data

    def nodes(self, kind: str | None = None) -> Iterator[Node]:
        # Materialise first: callers legitimately add/remove nodes while
        # iterating (e.g. cross-dir shadow-node creation), and the Rust backend
        # already returns a snapshot list — parity requires the same here.
        for _, attrs in list(self.g.nodes(data=True)):
            node: Node = attrs["data"]
            if kind is None or node.kind.value == kind:
                yield node

    def node_ids(self) -> list[str]:
        return list(self.g.nodes)

    def all_nodes(self) -> list[Node]:
        return [attrs["data"] for _, attrs in self.g.nodes(data=True)]

    def remove_node(self, node_id: str) -> bool:
        if not self.g.has_node(node_id):
            return False
        self.g.remove_node(node_id)
        return True

    def remove_nodes_batch(self, node_ids: list[str]) -> int:
        """Remove many nodes at once (networkx cleans up touching edges).
        Returns the count removed."""
        removed = 0
        for node_id in node_ids:
            if self.g.has_node(node_id):
                self.g.remove_node(node_id)
                removed += 1
        return removed

    def add_edge(self, edge: Edge) -> None:
        self.g.add_edge(edge.src, edge.dst, key=edge.type.value, data=edge)
        if self.bus is not None:
            self.bus.emit(
                "edge", src=edge.src, dst=edge.dst,
                edge_type=edge.type.value, weight=edge.weight,
            )

    def get_edge(self, src: str, dst: str, type: EdgeType) -> Edge | None:
        if not self.g.has_edge(src, dst, key=type.value):
            return None
        return self.g.edges[src, dst, type.value]["data"]

    def edges(self, type: EdgeType | None = None) -> Iterator[Edge]:
        # Materialise first (mutation-safe iteration; parity with Rust backend).
        for _, _, key, attrs in list(self.g.edges(data=True, keys=True)):
            edge: Edge = attrs["data"]
            if type is None or edge.type == type:
                yield edge

    def out_edges(self, node_id: str, type: EdgeType | None = None) -> Iterator[Edge]:
        for _, _, key, attrs in list(self.g.out_edges(node_id, data=True, keys=True)):
            edge: Edge = attrs["data"]
            if type is None or edge.type == type:
                yield edge

    def in_edges(self, node_id: str, type: EdgeType | None = None) -> Iterator[Edge]:
        for _, _, key, attrs in list(self.g.in_edges(node_id, data=True, keys=True)):
            edge: Edge = attrs["data"]
            if type is None or edge.type == type:
                yield edge

    def all_edges(self) -> list[Edge]:
        return [attrs["data"] for _, _, _, attrs in self.g.edges(data=True, keys=True)]

    def clear_edges(self, type: EdgeType) -> None:
        to_remove = [
            (u, v, k) for u, v, k, a in self.g.edges(data=True, keys=True)
            if a["data"].type == type
        ]
        self.g.remove_edges_from(to_remove)
        if to_remove and self.bus is not None:
            self.bus.emit(
                "reset_dim", edge_type=type.value, removed=len(to_remove),
            )

    def counts(self) -> dict[str, int]:
        by_type: dict[str, int] = {t.value: 0 for t in EdgeType}
        for edge in self.edges():
            by_type[edge.type.value] += 1
        return {
            "nodes": self.g.number_of_nodes(),
            "edges": self.g.number_of_edges(),
            **{f"edges_{k}": v for k, v in by_type.items()},
        }

    def signature(self) -> str:
        import hashlib
        h = hashlib.sha256()
        h.update(str(self.g.number_of_nodes()).encode())
        h.update(b"|")
        edge_tuples = sorted(
            (e.src, e.dst, e.type.value, round(e.weight, 6))
            for e in self.edges()
        )
        for tup in edge_tuples:
            h.update(repr(tup).encode())
            h.update(b";")
        return h.hexdigest()

    def attach_root(self, root: str) -> None:
        if root:
            self.roots.add(root)

    def merge(self, other: "PythonGraphStore") -> dict[str, Any]:
        if not self.root:
            for n in other.all_nodes():
                self.add_node(n)
            for e in other.all_edges():
                self.add_edge(e)
            return {"absorbed_nodes": len(other.all_nodes()),
                    "absorbed_edges": other.g.number_of_edges(),
                    "rekeyed": 0, "skipped": 0}

        my_root = Path(self.root).resolve()
        rekey: dict[str, str] = {}
        skipped = 0
        for n in other.all_nodes():
            try:
                rel = Path(n.path).resolve().relative_to(my_root)
            except (ValueError, OSError):
                skipped += 1
                continue
            new_id = "/".join(rel.parts)
            rekey[n.id] = new_id

        absorbed_nodes = 0
        for n in other.all_nodes():
            new_id = rekey.get(n.id)
            if new_id is None:
                continue
            merged = Node(
                id=new_id, path=n.path, kind=n.kind, size=n.size, mtime=n.mtime,
                mime=n.mime, content_hash=n.content_hash,
                root=n.root or other.root, attrs=dict(n.attrs),
            )
            existing = self.get_node(new_id)
            if existing is None:
                self.add_node(merged)
                absorbed_nodes += 1
            else:
                if n.root and n.root not in self.roots:
                    self.attach_root(n.root)

        absorbed_edges = 0
        for e in other.all_edges():
            new_src = rekey.get(e.src)
            new_dst = rekey.get(e.dst)
            if new_src is None or new_dst is None:
                continue
            if self.get_edge(new_src, new_dst, e.type) is not None:
                continue
            self.add_edge(Edge(
                src=new_src, dst=new_dst, type=e.type,
                weight=e.weight, attrs=dict(e.attrs),
            ))
            absorbed_edges += 1

        if other.root:
            self.attach_root(other.root)
        for r in other.roots:
            self.attach_root(r)

        summary = {"absorbed_nodes": absorbed_nodes, "absorbed_edges": absorbed_edges,
                   "rekeyed": len(rekey), "skipped": skipped}
        if self.bus is not None:
            self.bus.emit("merge", parent=self.root, absorbed=other.root, **summary)
        return summary


# ---- Rust-backed store (preferred) ---------------------------------------

class _RustBackedStore:
    """GraphStore backed by RustGraphStore. Data lives in Rust; events in Python."""

    def __init__(self, root: str = "", bus: EventBus | None = None,
                 _rust=None) -> None:
        self._rust = _rust or _try_rust()
        self.bus = bus
        self._inner = self._rust.RustGraphStore(root)
        # root/roots are properties proxying to the Rust store, but we also
        # keep meta on the Python side (it's small and rarely hot).
        self.meta: dict[str, Any] = {}
        # Python-side memo copy — the persistent transport (SQLite memo table).
        # The Rust passes consult their own native maps; seeding/harvesting
        # between the two sides happens in bulk around every Rust pass call
        # (_rust_memos_push / _rust_memos_pull, see engine.py + core.py).
        self.memo: dict[str, dict] = {"refs": {}, "simhash": {}, "embed": {}}

    # ---- root / roots (proxied to Rust) ----
    @property
    def root(self) -> str:
        return self._inner.root

    @root.setter
    def root(self, value: str) -> None:
        self._inner.root = value

    # ---- content-addressed memo transport (bulk, Python <-> Rust) ----
    def _rust_memos_push(self) -> None:
        """Seed the Rust store's native memo maps from this Python-side memo.

        Called before every Rust pass that consumes memos; the JSON round-trip
        is one bulk crossing per pass, never per file.
        """
        self._inner.set_memos(json.dumps(self.memo))

    def _rust_memos_pull(self) -> None:
        """Harvest the Rust store's native memo maps back into ``self.memo``.

        Called after every Rust pass that may have extended them. Embed dims
        come back as JSON string keys and are restored to int keys so reused
        vectors interoperate with freshly-computed ones (dict-key identity in
        the cosine/inverted-index paths).
        """
        raw = json.loads(self._inner.get_memos() or "{}")
        embed_raw = raw.get("embed", {}) or {}
        embed: dict[str, dict] = {}
        for h, vec in embed_raw.items():
            if isinstance(vec, dict):
                embed[h] = {
                    int(k): float(v) for k, v in vec.items()
                    if isinstance(v, (int, float))
                }
            else:
                embed[h] = vec
        self.memo = {
            "refs": raw.get("refs", {}) or {},
            "simhash": raw.get("simhash", {}) or {},
            "embed": embed,
        }

    @property
    def roots(self) -> set[str]:
        return set(self._inner.roots)

    @roots.setter
    def roots(self, value) -> None:
        self._inner.roots = list(value) if isinstance(value, (set, list, tuple)) else value

    # ---- nodes ----
    def add_node(self, node: Node) -> None:
        is_new = self._inner.add_node(node.to_dict())
        if is_new and self.bus is not None:
            self.bus.emit(
                "node", id=node.id, node_kind=node.kind.value,
                path=node.path, size=node.size,
            )

    def has_node(self, node_id: str) -> bool:
        return self._inner.has_node(node_id)

    def get_node(self, node_id: str) -> Node | None:
        raw = self._inner.get_node(node_id)
        if raw is None:
            return None
        return Node.from_dict(json.loads(raw))

    def nodes(self, kind: str | None = None) -> Iterator[Node]:
        for raw in self._inner.all_nodes():
            n = Node.from_dict(json.loads(raw))
            if kind is None or n.kind.value == kind:
                yield n

    def node_ids(self) -> list[str]:
        return self._inner.node_ids()

    def all_nodes(self) -> list[Node]:
        return [Node.from_dict(json.loads(r)) for r in self._inner.all_nodes()]

    def remove_node(self, node_id: str) -> bool:
        return self._inner.remove_node(node_id)

    def remove_nodes_batch(self, node_ids: list[str]) -> int:
        """Remove many nodes at once — one index cleanup for the whole batch."""
        return self._inner.remove_nodes_batch(list(node_ids))

    # ---- edges ----
    def add_edge(self, edge: Edge) -> None:
        self._inner.add_edge({
            "src": edge.src, "dst": edge.dst, "edge_type": edge.type.value,
            "weight": edge.weight, "attrs": edge.attrs,
        })
        if self.bus is not None:
            self.bus.emit(
                "edge", src=edge.src, dst=edge.dst,
                edge_type=edge.type.value, weight=edge.weight,
            )

    def get_edge(self, src: str, dst: str, type: EdgeType) -> Edge | None:
        raw = self._inner.get_edge(src, dst, type.value)
        if raw is None:
            return None
        d = json.loads(raw)
        return Edge(src=d["src"], dst=d["dst"], type=EdgeType(d["edge_type"]),
                    weight=d.get("weight", 1.0), attrs=d.get("attrs", {}))

    def edges(self, type: EdgeType | None = None) -> Iterator[Edge]:
        type_val = type.value if type is not None else None
        for raw in self._inner.all_edges():
            d = json.loads(raw)
            e = Edge(src=d["src"], dst=d["dst"], type=EdgeType(d["edge_type"]),
                     weight=d.get("weight", 1.0), attrs=d.get("attrs", {}))
            if type is None or e.type == type:
                yield e

    def out_edges(self, node_id: str, type: EdgeType | None = None) -> Iterator[Edge]:
        # O(degree): served by the Rust endpoint index ("" = all types).
        type_val = type.value if type is not None else ""
        for raw in self._inner.out_edges(node_id, type_val):
            d = json.loads(raw)
            yield Edge(src=d["src"], dst=d["dst"], type=EdgeType(d["edge_type"]),
                       weight=d.get("weight", 1.0), attrs=d.get("attrs", {}))

    def in_edges(self, node_id: str, type: EdgeType | None = None) -> Iterator[Edge]:
        # O(degree): served by the Rust endpoint index ("" = all types).
        type_val = type.value if type is not None else ""
        for raw in self._inner.in_edges(node_id, type_val):
            d = json.loads(raw)
            yield Edge(src=d["src"], dst=d["dst"], type=EdgeType(d["edge_type"]),
                       weight=d.get("weight", 1.0), attrs=d.get("attrs", {}))

    def all_edges(self) -> list[Edge]:
        return list(self.edges())

    def clear_edges(self, type: EdgeType) -> None:
        removed = self._inner.clear_edges(type.value)
        if removed and self.bus is not None:
            self.bus.emit("reset_dim", edge_type=type.value, removed=removed)

    # ---- stats / convergence ----
    def counts(self) -> dict[str, int]:
        return json.loads(self._inner.counts())

    def signature(self) -> str:
        return self._inner.signature()

    # ---- multi-root ----
    def attach_root(self, root: str) -> None:
        self._inner.attach_root(root)

    def merge(self, other: "_RustBackedStore") -> dict[str, Any]:
        # other may be a _RustBackedStore (has _inner) or a PythonGraphStore.
        if isinstance(other, _RustBackedStore):
            summary = json.loads(self._inner.merge(other._inner))
        else:
            # Fallback: other is a PythonGraphStore. Convert to a snapshot and
            # build a temporary Rust store, then merge that.
            tmp = self._rust.RustGraphStore(other.root)
            for n in other.all_nodes():
                tmp.add_node(n.to_dict())
            for e in other.all_edges():
                tmp.add_edge({"src": e.src, "dst": e.dst, "edge_type": e.type.value,
                              "weight": e.weight, "attrs": e.attrs})
            summary = json.loads(self._inner.merge(tmp))
        if self.bus is not None:
            self.bus.emit("merge", parent=self.root, absorbed=getattr(other, "root", ""),
                          **summary)
        return summary

    # ---- snapshot (for load/save + dispatch ops) ----
    def to_snapshot(self) -> dict:
        return json.loads(self._inner.to_snapshot())

    def load_snapshot(self, snap: dict) -> None:
        self._inner.load_snapshot(snap)


# ---- the public facade ---------------------------------------------------

# Module-level: pick the Rust backend once. Tests that set prefer_rust=False
# construct PythonGraphStore directly; the daemon/Core use GraphStore.
_RUST = _try_rust()


def GraphStore(root: str = "", bus: EventBus | None = None):
    """Construct a GraphStore. Uses the Rust backend if available, else Python.

    Returns either a ``_RustBackedStore`` or a ``PythonGraphStore``; both
    implement the same API. Callers should treat the return value as the
    GraphStore interface (duck-typed).
    """
    if _RUST is not None:
        return _RustBackedStore(root=root, bus=bus, _rust=_RUST)
    return PythonGraphStore(root=root, bus=bus)
