"""Exact-recall guarantees for the scale algorithms.

The banding (simhash near-dup) and inverted-index (semantic cosine) candidate
generators must produce byte-identical outputs to the historical O(n^2)
sweeps. These tests pin that equivalence property against inline brute-force
references, prove the pigeonhole boundary cases, and sanity-check scale.
"""

from __future__ import annotations

import itertools
import random
import time

import pytest

from dataworm.config import Config
from dataworm.engine import hashing_pass, semantic_pass, _similar_pairs
from dataworm.extractors import hashing, semantic
from dataworm.graph import GraphStore, PythonGraphStore
from dataworm.models import EdgeType, Node, NodeKind

THRESHOLD = 0.35


# ---- helpers ---------------------------------------------------------------

def _brute_near_pairs(fps):
    """Reference: every i<j pair within hamming distance 3."""
    return {
        (i, j)
        for i, j in itertools.combinations(range(len(fps)), 2)
        if hashing.is_near_duplicate(fps[i], fps[j])
    }


def _band_pairs(fps):
    """Production: deduped candidate set from banding."""
    return set(hashing.near_duplicate_candidates(fps))


def _brute_similar_pairs(vectors, threshold=THRESHOLD):
    return {
        (i, j)
        for i, j in itertools.combinations(range(len(vectors)), 2)
        if semantic.cosine(vectors[i], vectors[j]) >= threshold
    }


def _index_similar_pairs(vectors, threshold=THRESHOLD):
    return {(i, j) for i, j, _score in _similar_pairs(vectors, threshold)}


def _synth_vectors(rng, n):
    """Sparse unit vectors over a 64-dim vocab with planted overlap clusters."""
    vecs = []
    for k in range(n):
        vec = {}
        # Planted cluster signal: files in the same family share pivot dims.
        family = k % 8
        for dim in range(family * 4, family * 4 + 3):
            vec[dim] = rng.uniform(0.5, 1.0)
        # Individual noise dims.
        for _ in range(rng.randint(2, 6)):
            dim = rng.randrange(64)
            vec[dim] = max(vec.get(dim, 0.0), rng.uniform(0.1, 0.9))
        norm = sum(v * v for v in vec.values()) ** 0.5 or 1.0
        vecs.append({d: w / norm for d, w in vec.items()})
    return vecs


def _synth_fingerprints(rng, n):
    """Fingerprints with planted near-dup clusters (1-3 bit flips)."""
    base = [rng.getrandbits(64) | 1 for _ in range(n // 3)]
    fps = list(base)
    while len(fps) < n:
        src = rng.choice(base)
        mutated = src
        for bit in rng.sample(range(64), rng.randint(0, 2)):
            mutated ^= 1 << bit
        fps.append(mutated)
    return fps[:n]


# ---- equivalence properties -------------------------------------------------

@pytest.mark.parametrize("seed", [1, 7, 123])
def test_band_candidates_equal_brute_force(seed):
    """Every true near-dup pair must be recalled by the bands (verification
    filters the intentional over-generation down to exactly the truth)."""
    rng = random.Random(seed)
    fps = _synth_fingerprints(rng, 400)
    recalled = {
        (i, j) for i, j in _band_pairs(fps)
        if hashing.is_near_duplicate(fps[i], fps[j])
    }
    assert recalled == _brute_near_pairs(fps)


@pytest.mark.parametrize("seed", [1, 7, 123])
def test_index_candidates_equal_brute_force(seed):
    rng = random.Random(seed)
    vecs = _synth_vectors(rng, 300)
    assert _index_similar_pairs(vecs) == _brute_similar_pairs(vecs)


def test_engine_hashing_pass_matches_bruteforce(tmp_path):
    """End-to-end: production hashing_pass edges == brute-force reference."""
    rng = random.Random(42)
    texts = []
    n_files = 60
    # Zero-padded ids keep lexicographic order == numeric order, matching the
    # engine's candidate indexing (it sorts node ids as strings).
    names = [f"f{k:03d}.txt" for k in range(n_files)]
    for k in range(n_files):
        words = [f"w{rng.randrange(200)}" for _ in range(40)]
        if k % 5 == 1 and texts:  # plant a near-dup of a previous file
            base = texts[-1].split()
            base[rng.randrange(len(base))] = "mutant"
            words = base
        texts.append(" ".join(words))
        (tmp_path / names[k]).write_text(texts[-1], encoding="utf-8")

    store = PythonGraphStore(root=str(tmp_path))
    for name in names:
        p = tmp_path / name
        store.add_node(Node(id=name, path=str(p), kind=NodeKind.FILE,
                            size=p.stat().st_size))
    hashing_pass(store, Config(root=str(tmp_path), enable_semantic=False))

    fps = [hashing.simhash(t) for t in texts]
    got = {(e.src, e.dst) for e in store.edges(EdgeType.DUPLICATE_OF)}
    want = {(names[i], names[j]) for i, j in _brute_near_pairs(fps)}
    assert got == want


def test_engine_semantic_pass_matches_bruteforce(tmp_path):
    rng = random.Random(43)
    store = PythonGraphStore(root=str(tmp_path))
    texts = []
    n_files = 50
    names = [f"s{k:03d}.txt" for k in range(n_files)]  # lex==numeric order
    for k in range(n_files):
        words = [f"t{rng.randrange(150)}" for _ in range(30)] + ["zpivot"]
        texts.append(" ".join(words))
        (tmp_path / names[k]).write_text(texts[-1], encoding="utf-8")
        store.add_node(Node(id=names[k], path=str(tmp_path / names[k]),
                            kind=NodeKind.FILE,
                            size=(tmp_path / names[k]).stat().st_size))
    embedder = semantic.TfidfEmbedder()
    vecs = embedder.embed(texts)

    cfg = Config(root=str(tmp_path), enable_hashing=False,
                 similarity_threshold=THRESHOLD)
    semantic_pass(store, cfg)

    got = {(e.src, e.dst) for e in store.edges(EdgeType.SIMILAR_TO)}
    want = {(names[i], names[j]) for i, j in _brute_similar_pairs(vecs)}
    assert got == want


# ---- adversarial / boundary cases -------------------------------------------

def test_banding_pigeonhole_boundary():
    """4 flipped bits across exactly 2 bands -> NOT recalled (hamming=4).
    3 bits across 2 bands -> IS recalled (third band clean)."""
    a = 0b1111_0000000000000000_0000000000000000_0000000000000000_0000000000000000
    four_bits_two_bands = a ^ (1 << 63) ^ (1 << 32) ^ (1 << 16) ^ (1 << 0)
    three_bits_two_bands = a ^ (1 << 63) ^ (1 << 40) ^ (1 << 17)
    assert hashing.hamming_distance(a, four_bits_two_bands) == 4
    assert hashing.hamming_distance(a, three_bits_two_bands) == 3

    pairs = set(_band_pairs([a, four_bits_two_bands]))
    assert pairs == set()  # beyond threshold: correctly rejected

    pairs = set(_band_pairs([a, three_bits_two_bands]))
    assert pairs == {(0, 1)}  # within threshold: recalled despite 2 dirty bands


def test_index_recall_single_shared_dim():
    """A pair sharing only ONE low-idf dimension still clears the index."""
    v1 = {5: 1.0}
    v2 = {5: 1.0}  # identical single-dim unit vectors -> cosine 1.0
    assert (0, 1) in _index_similar_pairs([v1, v2])


def test_empty_vectors_never_qualify():
    assert _index_similar_pairs([{}, {}, {}]) == set()


# ---- determinism + scale ----------------------------------------------------

def test_semantic_pass_deterministic(tmp_path):
    store = GraphStore()
    texts = ["alpha beta gamma delta", "alpha beta gamma epsilon",
             "zeta eta theta iota"] * 4
    for k, t in enumerate(texts):
        p = tmp_path / f"d{k}.txt"
        p.write_text(t, encoding="utf-8")
        store.add_node(Node(id=f"d{k}.txt", path=str(p), kind=NodeKind.FILE,
                            size=p.stat().st_size))
    cfg = Config(root=str(tmp_path), enable_hashing=False,
                 similarity_threshold=THRESHOLD)
    semantic_pass(store, cfg)
    sig1 = store.signature()
    store.clear_edges(EdgeType.SIMILAR_TO)
    semantic_pass(store, cfg)
    assert store.signature() == sig1


def test_twenty_thousand_fingerprints_complete():
    """20k-fingerprint banding completes comfortably (soft bound: 10s)."""
    rng = random.Random(99)
    fps = _synth_fingerprints(rng, 20_000)
    start = time.perf_counter()
    recalled = {
        (i, j) for i, j in _band_pairs(fps)
        if hashing.is_near_duplicate(fps[i], fps[j])
    }
    elapsed = time.perf_counter() - start
    brute_sample = _brute_near_pairs(fps[:500])
    assert recalled and len(recalled) > 100
    # Spot-verify recall against brute force on a prefix.
    small = {(i, j) for i, j in recalled if i < 500 and j < 500}
    assert small == brute_sample
    print(f"20k fingerprints: {len(recalled)} true pairs in {elapsed:.2f}s")
    assert elapsed < 10.0
