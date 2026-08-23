"""Agent-facing query API over a built graph.

The headline capability is :meth:`QueryAPI.impact_of` (a.k.a. blast radius):
given a file, it returns everything that transitively *depends on* it via
``references`` edges — so an agent can see what an edit would break and
re-confirm before proceeding.

When the underlying store is Rust-backed, every query runs in Rust over the
graph held in Rust memory (the Python wrapper just (de)serialises the small
result). When the store is the pure-Python fallback, the queries run in Python
(networkx) — kept for ``--no-rust`` and the parity tests.
"""

from __future__ import annotations

import json
from collections import deque
from pathlib import Path
from typing import Iterable

from dataworm.graph import GraphStore
from dataworm.models import EdgeType, NodeKind

_ALL_TYPES = tuple(EdgeType)


def _is_rust(store) -> bool:
    return hasattr(store, "_inner") and hasattr(store, "_rust")


class QueryAPI:
    def __init__(self, store: GraphStore) -> None:
        self.store = store

    # ---- path normalisation ---------------------------------------------
    def to_id(self, path: str) -> str | None:
        """Map a user-supplied path (id, relative, or absolute) to a node id."""
        if _is_rust(self.store):
            return self.store._inner.to_id(path)
        return self._to_id_py(path)

    def _to_id_py(self, path: str) -> str | None:
        if self.store.has_node(path):
            return path
        root = self.store.root
        if root:
            try:
                rel = Path(path).resolve().relative_to(Path(root).resolve())
                cand = "/".join(rel.parts)
                if self.store.has_node(cand):
                    return cand
            except (ValueError, OSError):
                pass
        # Fall back to a suffix match on the id (handles cwd-relative input).
        needle = path.replace("\\", "/").lstrip("./")
        for node_id in self.store.node_ids():
            if node_id == needle or node_id.endswith("/" + needle):
                return node_id
        return None

    # ---- blast radius ----------------------------------------------------
    def impact_of(self, path: str) -> dict:
        """Everything that would be affected by editing ``path``.

        Walks ``references`` edges *backwards*: direct dependents are files that
        reference ``path``; transitive dependents reference those, and so on.
        """
        if _is_rust(self.store):
            return json.loads(self.store._inner.impact_of(path))
        return self._impact_of_py(path)

    def _impact_of_py(self, path: str) -> dict:
        node_id = self.to_id(path)
        if node_id is None:
            return {"error": f"unknown path: {path}", "direct": [], "transitive": []}

        direct: list[str] = []
        transitive: list[str] = []
        seen: set[str] = {node_id}
        frontier: deque[tuple[str, int]] = deque([(node_id, 0)])

        while frontier:
            current, depth = frontier.popleft()
            for edge in self.store.in_edges(current, EdgeType.REFERENCES):
                src = edge.src
                if src in seen:
                    continue
                seen.add(src)
                if depth == 0:
                    direct.append(src)
                else:
                    transitive.append(src)
                frontier.append((src, depth + 1))

        direct = sorted(direct)
        transitive = sorted(transitive)
        # Cap response sizes (parity with the Rust backend).
        IMPACT_CAP = 1000
        direct_trunc = len(direct) > IMPACT_CAP
        trans_trunc = len(transitive) > IMPACT_CAP
        direct = direct[:IMPACT_CAP]
        transitive = transitive[:IMPACT_CAP]
        return {
            "target": node_id,
            "direct": direct,
            "transitive": transitive,
            "total_affected": len(direct) + len(transitive),
            "truncated": direct_trunc or trans_trunc,
        }

    # alias
    blast_radius = impact_of

    # ---- neighbourhood ---------------------------------------------------
    def neighbors(
        self,
        path: str,
        edge_types: Iterable[EdgeType] | None = None,
        depth: int = 1,
    ) -> dict:
        """Nodes within ``depth`` hops of ``path`` over the given edge types."""
        types_list = [t.value for t in edge_types] if edge_types else []
        if _is_rust(self.store):
            return json.loads(self.store._inner.neighbors(path, types_list, depth))
        return self._neighbors_py(path, edge_types, depth)

    def _neighbors_py(self, path, edge_types, depth) -> dict:
        node_id = self.to_id(path)
        if node_id is None:
            return {"error": f"unknown path: {path}", "neighbors": []}
        types = tuple(edge_types) if edge_types else _ALL_TYPES

        adjacency: dict[str, set[str]] = {}
        for edge in self.store.edges():
            if edge.type not in types:
                continue
            adjacency.setdefault(edge.src, set()).add(edge.dst)
            adjacency.setdefault(edge.dst, set()).add(edge.src)

        seen: dict[str, int] = {node_id: 0}
        frontier: deque[str] = deque([node_id])
        while frontier:
            current = frontier.popleft()
            if seen[current] >= depth:
                continue
            for nxt in adjacency.get(current, ()):  # type: ignore[arg-type]
                if nxt not in seen:
                    seen[nxt] = seen[current] + 1
                    frontier.append(nxt)

        result = [
            {"id": nid, "depth": d}
            for nid, d in sorted(seen.items())
            if nid != node_id
        ]
        # Cap the response (parity with the Rust backend).
        NEIGHBORS_CAP = 1000
        total = len(result)
        truncated = total > NEIGHBORS_CAP
        result = result[:NEIGHBORS_CAP]
        return {"target": node_id, "depth": depth, "neighbors": result,
                "truncated": truncated, "total": total}

    # ---- rich context ----------------------------------------------------
    def context_for(self, path: str) -> dict:
        """Full context bundle for a node: metadata + links across all dimensions."""
        if _is_rust(self.store):
            return json.loads(self.store._inner.context_for(path))
        return self._context_for_py(path)

    def _context_for_py(self, path: str) -> dict:
        node_id = self.to_id(path)
        if node_id is None:
            return {"error": f"unknown path: {path}"}
        node = self.store.get_node(node_id)
        assert node is not None

        def collect(edges) -> list[dict]:
            return [
                {"id": e.dst if e.src == node_id else e.src,
                 "type": e.type.value, "weight": round(e.weight, 4),
                 "direction": "out" if e.src == node_id else "in"}
                for e in edges
            ]

        links = collect(list(self.store.out_edges(node_id)) + list(self.store.in_edges(node_id)))
        by_type: dict[str, int] = {}
        for link in links:
            by_type[link["type"]] = by_type.get(link["type"], 0) + 1

        return {
            "node": node.to_dict(),
            "link_counts": by_type,
            "links": links,
            "dangling_references": node.attrs.get("dangling", []),
            "impact": self.impact_of(node_id),
        }

    # ---- search ----------------------------------------------------------
    def search(self, text: str, limit: int = 50) -> list[dict]:
        """Case-insensitive substring match against node ids/paths.

        Results are sorted by id for deterministic order across backends.
        The limit is clamped server-side (max 500) so a client can't request
        unbounded results.
        """
        MAX_SEARCH_LIMIT = 500
        limit = min(limit, MAX_SEARCH_LIMIT)
        if _is_rust(self.store):
            return json.loads(self.store._inner.search(text, limit))["results"]
        needle = text.lower().replace("\\", "/")
        hits: list[dict] = []
        for node in self.store.nodes():
            if needle in node.id.lower() or needle in node.path.lower():
                hits.append({"id": node.id, "kind": node.kind.value, "path": node.path})
        hits.sort(key=lambda h: h["id"])
        return hits[:limit]

    # ---- summary ---------------------------------------------------------
    def summary(self) -> dict:
        if _is_rust(self.store):
            return json.loads(self.store._inner.summary())
        counts = self.store.counts()
        kinds = {"dir": 0, "file": 0}
        for node in self.store.nodes():
            kinds[node.kind.value] += 1
        return {
            "root": self.store.root,
            "meta": self.store.meta,
            "node_kinds": kinds,
            **counts,
        }
