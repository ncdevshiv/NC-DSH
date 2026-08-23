from dataworm.crawler import crawl, _root_id
from dataworm.graph import GraphStore
from dataworm.models import EdgeType, NodeKind
from pathlib import Path


def test_crawl_creates_nodes_and_contains_edges(sample_config):
    store = GraphStore()
    crawl(store, sample_config)

    # Root node exists under its fragment-unique id (crawler resolves root).
    rid = _root_id(Path(sample_config.root).resolve())
    root = store.get_node(rid)
    assert root is not None and root.kind == NodeKind.DIR

    # Files and dirs discovered downward.
    for expected in ("a.py", "b.py", "c.py", "utils", "utils/helper.py", "docs/readme.md"):
        assert store.has_node(expected), expected

    # Structural edge from root to a top-level file.
    assert store.get_edge(rid, "a.py", EdgeType.CONTAINS) is not None
    # Structural edge into a subfolder and its file.
    assert store.get_edge(rid, "utils", EdgeType.CONTAINS) is not None
    assert store.get_edge("utils", "utils/helper.py", EdgeType.CONTAINS) is not None


def test_crawl_skips_ignored_dirs(sample_config):
    store = GraphStore()
    crawl(store, sample_config)
    assert not store.has_node("__pycache__")
    assert not store.has_node("__pycache__/junk.pyc")


def test_crawl_records_file_metadata(sample_config):
    store = GraphStore()
    crawl(store, sample_config)
    node = store.get_node("a.py")
    assert node.kind == NodeKind.FILE
    assert node.size > 0
    assert node.content_hash  # sha256 populated for small text files
