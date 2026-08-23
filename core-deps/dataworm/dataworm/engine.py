"""Engine: the multi-pass convergence loop.

Each cycle runs four passes that add/refine a dimension of the graph:

    crawl_pass      -> nodes + `contains`      (structure)
    reference_pass  -> `references`            (content links)
    hashing_pass    -> `duplicate_of`          (exact + near duplicates)
    semantic_pass   -> `similar_to`            (embedding similarity)

The cycle repeats until the graph's signature stops changing (a fixed point)
or ``max_cycles`` is reached — this is the worm "realigning over and over until
it has detailed context". Newly discovered nodes can resolve references that
were dangling in an earlier cycle, which is why more than one pass may matter.
"""

from __future__ import annotations

import json
from collections import defaultdict
from pathlib import Path, PurePosixPath

from dataworm.config import Config
from dataworm.crawler import crawl
from dataworm.extractors import hashing, references, semantic
from dataworm.graph import GraphStore
from dataworm.models import Edge, EdgeType, Node, NodeKind


# ---- helpers -------------------------------------------------------------

def _get_text(node: Node, config: Config) -> str:
    """Read a file's text, caching it transiently on the node for the cycle."""
    if node.kind != NodeKind.FILE or not config.is_text(node.id):
        return ""
    if node.size > config.max_content_bytes:
        return ""
    cached = node.attrs.get("_text")
    if cached is not None:
        return cached
    try:
        with open(node.path, "r", encoding="utf-8", errors="ignore") as fh:
            text = fh.read(config.max_content_bytes)
    except OSError:
        text = ""
    node.attrs["_text"] = text
    return text


# ---- content-addressed memo ----------------------------------------------
#
# Pass outputs are memoized by sha256 ``content_hash`` — the crawler recomputes
# that hash whenever a file's mtime+size change, so the hash IS the dirty
# marker: a memo hit means "this exact content was already extracted/embedded".
# Only EXTRACTION and EMBEDDING are memoized; edge RESOLUTION always re-runs
# against the current store (resolution legitimately changes as nodes appear/
# disappear). The memo dict shape is {"refs": {}, "simhash": {}, "embed": {}};
# it lives on the store (graph.py), threads across cycles and process restarts
# (persist.py's `memo` table), and may be passed explicitly to any pass.
#
# Known trade-off: TfidfEmbedder computes IDF over the batch it is given, so a
# warm run with *some* misses embeds those misses against each other rather
# than the full corpus. Fully-warm (all-hit) runs reuse stored vectors exactly
# and stay bit-identical to cold output; partially-warm runs keep every reused
# vector byte-exact and only the freshly-embedded files' IDF context differs.

def _memo_or(store_memo_fallback, memo: dict | None) -> dict | None:
    """Explicit memo wins; else fall back to the store-owned memo."""
    return memo if memo is not None else store_memo_fallback


def _embed_candidates(candidates, config: Config, memo: dict | None,
                      embedder=None) -> list:
    """Embed candidate texts with content-addressed memo reuse.

    Only texts whose content_hash is absent from ``memo["embed"]`` reach the
    embedder (one batch call over just the misses) and the disk; hits reuse
    their stored vector byte-exactly and are never read. See the module-level
    memo note for the TF-IDF batch trade-off.
    """
    vectors: list = [None] * len(candidates)
    miss_idx: list[int] = []
    miss_texts: list[str] = []
    for i, node in enumerate(candidates):
        cached = None
        if memo is not None and node.content_hash:
            cached = memo["embed"].get(node.content_hash)
        if cached is not None:
            vectors[i] = cached
        else:
            miss_idx.append(i)
            miss_texts.append(_get_text(node, config))
    if miss_texts:
        if embedder is None:
            embedder = semantic.get_embedder(config)
        fresh = embedder.embed(miss_texts)
        for i, vec in zip(miss_idx, fresh):
            vectors[i] = vec
            if memo is not None and candidates[i].content_hash:
                memo["embed"][candidates[i].content_hash] = vec
    return vectors


def _memo_ref_key(node: Node) -> str:
    # Raw reference extraction depends on bytes (content_hash) AND on the file
    # extension (it selects which regex family runs), so the key carries both.
    suffix = PurePosixPath(node.id).suffix.lower()
    return f"{node.content_hash}|{suffix}"


# ---- passes --------------------------------------------------------------

def crawl_pass(store: GraphStore, config: Config) -> None:
    crawl(store, config)
    # Drop nodes whose path vanished since a previous cycle (stale).
    for node in list(store.all_nodes()):
        if node.id and not Path(node.path).exists():
            store.g.remove_node(node.id)


def reference_pass(store: GraphStore, config: Config, memo: dict | None = None) -> None:
    # Rust-backed path: the heavy loop (read text, extract, resolve, add edges)
    # runs entirely in Rust on the store held in Rust memory. The Rust pass
    # clears + adds internally and returns the diff; we emit the bus events
    # (reset_dim + per-edge) here so the live dashboard animates.
    if _store_is_rust(store):
        # Seed the Rust store's native memo maps, then harvest them back: the
        # Rust pass consults/extends them natively (same rules as the Python
        # fallback below) with one bulk JSON crossing per pass.
        store._rust_memos_push()
        result = json.loads(store._inner.reference_pass(
            config.max_content_bytes,
            sorted(config.text_extensions),
        ))
        store._rust_memos_pull()
        if result.get("removed") and store.bus is not None:
            store.bus.emit("reset_dim", edge_type=EdgeType.REFERENCES.value,
                           removed=result["removed"])
        if store.bus is not None:
            for src, dst in result.get("added_edges", []):
                store.bus.emit("edge", src=src, dst=dst,
                               edge_type=EdgeType.REFERENCES.value, weight=1.0)
        # Return the dangling map for the cross-link pass. (We can't write it
        # back onto a Rust-backed store — its Node objects are transient DTOs —
        # so the caller records it per fragment.)
        return result.get("dangling", {})

    # Pure-Python fallback path (parity reference).
    memo = _memo_or(getattr(store, "memo", None), memo)
    store.clear_edges(EdgeType.REFERENCES)
    dangling_map: dict[str, list[str]] = {}
    for node in store.nodes(kind=NodeKind.FILE.value):
        # Memo hit: reuse the raw references extracted from this exact content
        # (keyed by hash + extension) — skip the open()+parse entirely.
        raw_refs: list[str] | None = None
        if memo is not None and node.content_hash:
            raw_refs = memo["refs"].get(_memo_ref_key(node))
        if raw_refs is None:
            text = _get_text(node, config)
            if not text:
                continue
            raw_refs = references.extract_raw_references(node, text)
            if memo is not None and node.content_hash:
                memo["refs"][_memo_ref_key(node)] = raw_refs
        resolved: set[str] = set()
        dangling: list[str] = []
        for raw in raw_refs:
            target = references.resolve_reference(store, node, raw)
            if target and target != node.id:
                resolved.add(target)
            elif not target:
                dangling.append(raw)
        for target in resolved:
            store.add_edge(Edge(src=node.id, dst=target, type=EdgeType.REFERENCES))
        node.attrs["dangling"] = dangling
        if dangling:
            dangling_map[node.id] = dangling
    return dangling_map


def _store_is_rust(store) -> bool:
    """True if `store` is the Rust-backed GraphStore (has `_inner`)."""
    return hasattr(store, "_inner") and hasattr(store, "_rust")


def hashing_pass(store: GraphStore, config: Config, memo: dict | None = None) -> None:
    store.clear_edges(EdgeType.DUPLICATE_OF)

    # Exact duplicates: group files by content hash.
    by_hash: dict[str, list[str]] = defaultdict(list)
    file_nodes: list[Node] = []
    for node in store.nodes(kind=NodeKind.FILE.value):
        file_nodes.append(node)
        if node.content_hash:
            by_hash[node.content_hash].append(node.id)

    for content_hash, ids in by_hash.items():
        if len(ids) < 2:
            continue
        canonical = sorted(ids)[0]
        for other in sorted(ids)[1:]:
            store.add_edge(Edge(
                src=other, dst=canonical, type=EdgeType.DUPLICATE_OF,
                weight=1.0, attrs={"reason": "exact", "sha256": content_hash[:12]},
            ))

    # Near duplicates: simhash over text files, compare within hamming distance.
    # Fingerprint lookup is memoized by content_hash — a hit skips the read.
    memo = _memo_or(getattr(store, "memo", None), memo)
    fingerprints: list[tuple[str, int]] = []
    for node in file_nodes:
        fp: int | None = None
        if memo is not None and node.content_hash:
            fp = memo["simhash"].get(node.content_hash)
        if fp is None:
            text = _get_text(node, config)
            if not text:
                continue
            fp = hashing.simhash(text)
            if memo is not None and node.content_hash:
                memo["simhash"][node.content_hash] = fp
        node.attrs["simhash"] = fp
        fingerprints.append((node.id, fp))

    # Cap: sort by id (deterministic) and truncate to `max_hashing_nodes` —
    # parity with the Rust hashing_pass. The cap is a safety valve for
    # pathologically self-similar corpora, not a correctness limit: banding
    # below recalls every hamming<=3 pair exactly at any scale.
    fingerprints.sort(key=lambda t: t[0])
    fingerprints = fingerprints[: config.max_hashing_nodes]

    # Near-duplicate candidates via 16-bit banding (pigeonhole over 4 bands:
    # <=3 differing bits touch <=3 bands, so every true pair shares >=1 band).
    # Verification stays the exact hamming check; each candidate pair is
    # verified once even when several bands surface it.
    verified: dict[tuple[int, int], int] = {}
    for i, j in hashing.near_duplicate_candidates([fp for _, fp in fingerprints]):
        if (i, j) in verified:
            continue
        fp_a, fp_b = fingerprints[i][1], fingerprints[j][1]
        if hashing.is_near_duplicate(fp_a, fp_b):
            verified[(i, j)] = hashing.hamming_distance(fp_a, fp_b)

    # Emit in ascending (i, j) == lexicographic id-pair order, byte-identical
    # to the previous full nested loop's insertion order.
    for i, j in sorted(verified):
        id_a, id_b = fingerprints[i][0], fingerprints[j][0]
        # Skip pairs already linked as exact duplicates (either direction).
        if (store.get_edge(id_a, id_b, EdgeType.DUPLICATE_OF)
                or store.get_edge(id_b, id_a, EdgeType.DUPLICATE_OF)):
            continue
        store.add_edge(Edge(
            src=id_a, dst=id_b, type=EdgeType.DUPLICATE_OF,
            weight=0.9, attrs={"reason": "near", "hamming": verified[(i, j)]},
        ))


def _similar_pairs_full_sweep(vectors: list, threshold: float) -> list[tuple[int, int, float]]:
    """The historical O(n^2) sweep: every pair through ``cosine``."""
    out: list[tuple[int, int, float]] = []
    for i in range(len(vectors)):
        vi = vectors[i]
        for j in range(i + 1, len(vectors)):
            score = semantic.cosine(vi, vectors[j])
            if score >= threshold:
                out.append((i, j, score))
    return out


def _similar_pairs(vectors: list, threshold: float) -> list[tuple[int, int, float]]:
    """All pairs with ``cosine >= threshold`` as ``(i, j, score)``, i < j,
    in ascending (i, j) order — byte-identical to the full nested loop.

    Exactness: vectors are unit-normalised, so a pair clearing a positive
    threshold must share at least one nonzero-weight dimension; co-occurrence
    in a dimension's posting list is therefore an exact-recall candidate
    filter and each pair is still scored by the existing ``cosine``. For
    ``threshold <= 0`` no pruning is valid (two empty vectors score 0.0 and
    qualify), so the full sweep runs. When posting lists are so dense that
    candidates approach the full pairwise block (e.g. dense embeddings), the
    plain sweep streams with less memory and identical output.
    """
    n = len(vectors)
    if n < 2:
        return []
    if threshold <= 0.0:
        return _similar_pairs_full_sweep(vectors, threshold)

    postings: defaultdict[int, list[int]] = defaultdict(list)
    non_empty = 0
    for i, vec in enumerate(vectors):
        if vec:
            non_empty += 1
            for dim in vec:
                postings[dim].append(i)
    if non_empty < 2:
        return []

    brute_pairs = non_empty * (non_empty - 1) // 2
    co_occurrences = sum(len(m) * (len(m) - 1) // 2 for m in postings.values())
    if 2 * co_occurrences >= brute_pairs:
        # Index would visit >= half of all pairs anyway — sweep instead.
        return _similar_pairs_full_sweep(vectors, threshold)

    checked: dict[tuple[int, int], float] = {}
    for members in postings.values():
        if len(members) < 2:
            continue
        for a in range(len(members) - 1):
            i = members[a]
            for b in range(a + 1, len(members)):
                key = (i, members[b])
                if key not in checked:
                    score = semantic.cosine(vectors[i], vectors[members[b]])
                    if score >= threshold:
                        checked[key] = score
    return [(i, j, checked[(i, j)]) for i, j in sorted(checked)]


def semantic_pass(store: GraphStore, config: Config, memo: dict | None = None) -> None:
    if not config.enable_semantic:
        # Still clear so a disabled pass doesn't leave stale edges.
        if _store_is_rust(store):
            store._inner.clear_edges(EdgeType.SIMILAR_TO.value)
        else:
            store.clear_edges(EdgeType.SIMILAR_TO)
        return
    memo = _memo_or(getattr(store, "memo", None), memo)

    # Rust-backed path: the heavy loop (embed + pairwise cosine) runs in Rust.
    if _store_is_rust(store):
        embedder = semantic.get_embedder(config)
        if isinstance(embedder, semantic.TfidfEmbedder):
            # All-Rust: TF-IDF embed + cosine in one call. Seed/harvest the
            # native memo maps around it (the Rust pass memoizes embeddings by
            # content_hash exactly like _embed_candidates does below).
            store._rust_memos_push()
            result = json.loads(store._inner.semantic_pass(
                config.max_content_bytes,
                sorted(config.text_extensions),
                config.max_semantic_nodes,
                config.similarity_threshold,
            ))
            store._rust_memos_pull()
            _emit_semantic_events(store, result)
            return
        # sentence-transformers path: embed in Python (it's a Python lib),
        # then do the O(n^2) compare in Rust.
        candidates = sorted(
            (n for n in store.nodes(kind=NodeKind.FILE.value)
             if config.is_text(n.id) and n.size <= config.max_content_bytes),
            key=lambda n: n.id,
        )[: config.max_semantic_nodes]
        if len(candidates) < 2:
            return
        vectors = _embed_candidates(candidates, config, memo, embedder=embedder)
        # Hand vectors to Rust for the compare.
        items = [(candidates[i].id, vectors[i]) for i in range(len(candidates))]
        result = _semantic_pass_from_vectors_rust(store, items, config.similarity_threshold)
        _emit_semantic_events(store, result)
        return

    # Pure-Python fallback (parity reference).
    store.clear_edges(EdgeType.SIMILAR_TO)
    candidates = sorted(
        (n for n in store.nodes(kind=NodeKind.FILE.value) if config.is_text(n.id) and n.size <= config.max_content_bytes),
        key=lambda n: n.id,
    )[: config.max_semantic_nodes]
    if len(candidates) < 2:
        return

    vectors = _embed_candidates(candidates, config, memo)

    # Candidate generation via the exact-recall inverted index (see
    # ``_similar_pairs``): pairs arrive ascending (i, j) with score >=
    # threshold, so insertion order and weights are identical to the old
    # nested i<j cosine loop this replaces.
    for i, j, score in _similar_pairs(vectors, config.similarity_threshold):
        store.add_edge(Edge(
            src=candidates[i].id, dst=candidates[j].id,
            type=EdgeType.SIMILAR_TO, weight=round(score, 6),
        ))


def _emit_semantic_events(store, result: dict) -> None:
    """Emit reset_dim + per-edge bus events from a Rust semantic_pass result."""
    if store.bus is not None:
        # clear_edges already ran inside Rust; emit the reset_dim for the UI.
        store.bus.emit("reset_dim", edge_type=EdgeType.SIMILAR_TO.value,
                       removed=result.get("removed", 0))
        for entry in result.get("added_edges", []):
            # entry is [src, dst, weight]
            src, dst = entry[0], entry[1]
            w = entry[2] if len(entry) > 2 else 1.0
            store.bus.emit("edge", src=src, dst=dst,
                           edge_type=EdgeType.SIMILAR_TO.value, weight=w)


def _semantic_pass_from_vectors_rust(store, items, threshold):
    """Call the Rust semantic_pass_from_vectors via dispatch (stateless op)."""
    # Build the params in the shape Rust expects: list of {id, vec}.
    # We use the dispatch path since the stateful method takes the raw vectors.
    # The Rust store's from_vectors isn't exposed as a pymethod directly; use
    # dispatch for parity with the standalone binary contract.
    import dataworm._rust as rust
    # Serialize vectors (dict[int,float]) -> JSON-safe {str(idx): weight}.
    payload_items = [
        {"id": nid, "vec": {str(k): v for k, v in vec.items()}}
        for nid, vec in items
    ]
    # We need to run on *this* store, not a fresh one. Use a snapshot + apply.
    snap = store.to_snapshot()
    snap_meta = dict(snap.get("meta", {}))
    snap_meta["_semantic_items"] = payload_items
    snap_meta["_threshold"] = threshold
    result = rust.dispatch("semantic_pass_from_vectors", {
        "snapshot": snap, "items": payload_items, "threshold": threshold,
    })
    # Apply the added edges to the live store. Clear its SIMILAR_TO dimension
    # FIRST: the Rust compare ran against a throwaway snapshot (clearing only
    # that copy), so without this, pairs whose score dropped below threshold
    # would keep their stale edges forever.
    store.clear_edges(EdgeType.SIMILAR_TO)
    for entry in result.get("added_edges", []):
        src, dst, w = entry[0], entry[1], entry[2] if len(entry) > 2 else 1.0
        store.add_edge(Edge(src=src, dst=dst, type=EdgeType.SIMILAR_TO, weight=w))
    return result


# ---- orchestration -------------------------------------------------------

def run(config: Config, max_cycles: int = 5, bus=None) -> GraphStore:
    """Crawl + realign until the graph converges. Returns the finished store."""
    store = GraphStore(root=config.root, bus=bus)
    if bus is not None:
        bus.emit("start", root=config.root, max_cycles=max_cycles)
    prev_signature: str | None = None
    cycles = 0
    converged = False

    for cycle in range(max_cycles):
        cycles = cycle + 1
        # Lifecycle events: real passes as they run.
        for name, fn in [
            ("crawl", crawl_pass),
            ("references", reference_pass),
            ("hashing", hashing_pass),
            ("semantic", semantic_pass),
        ]:
            if bus is not None:
                bus.emit("pass", name=name, cycle=cycles, status="start")
            fn(store, config)
            if bus is not None:
                bus.emit("pass", name=name, cycle=cycles, status="end")

        if bus is not None:
            bus.emit("cycle", n=cycles, signature=store.signature())

        signature = store.signature()
        if signature == prev_signature:
            converged = True
            break
        prev_signature = signature

    # Strip transient per-cycle caches before the graph is consumed/persisted.
    for node in store.all_nodes():
        node.attrs.pop("_text", None)

    counts = store.counts()
    store.meta.update({
        "root": store.root,
        "cycles": cycles,
        "converged": converged,
        "max_cycles": max_cycles,
    })
    if bus is not None:
        bus.emit("done", converged=converged, cycles=cycles,
                 counts=counts, root=store.root)
    return store
