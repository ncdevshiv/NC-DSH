from dataworm.models import EdgeType


def test_engine_produces_all_edge_types(sample_store):
    counts = sample_store.counts()
    assert counts["edges_contains"] > 0
    assert counts["edges_references"] > 0
    assert counts["edges_duplicate_of"] > 0
    # Semantic dimension is on by default (TF-IDF fallback).
    assert counts["edges_similar_to"] >= 0


def test_engine_converges_to_fixed_point(sample_store):
    assert sample_store.meta["converged"] is True
    assert sample_store.meta["cycles"] >= 2  # needs a 2nd cycle to confirm stability


def test_reference_edges_present(sample_store):
    # a.py imports b.py.
    assert sample_store.get_edge("a.py", "b.py", EdgeType.REFERENCES) is not None
    # b.py imports c.py.
    assert sample_store.get_edge("b.py", "c.py", EdgeType.REFERENCES) is not None
    # readme links guide.
    assert sample_store.get_edge("docs/readme.md", "docs/guide.md", EdgeType.REFERENCES) is not None


def test_exact_duplicate_edge(sample_store):
    edge = sample_store.get_edge("dup2.txt", "dup1.txt", EdgeType.DUPLICATE_OF)
    assert edge is not None
    assert edge.attrs.get("reason") == "exact"


def test_signature_is_stable_across_extra_cycle(sample_config, sample_store):
    from dataworm.engine import (crawl_pass, hashing_pass, reference_pass,
                                 semantic_pass)

    before = sample_store.signature()
    # Run another full cycle in place; nothing should change.
    crawl_pass(sample_store, sample_config)
    reference_pass(sample_store, sample_config)
    hashing_pass(sample_store, sample_config)
    semantic_pass(sample_store, sample_config)
    assert sample_store.signature() == before
