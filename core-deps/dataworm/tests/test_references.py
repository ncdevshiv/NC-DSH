from dataworm.crawler import crawl
from dataworm.extractors import references
from dataworm.graph import GraphStore
from dataworm.models import Node, NodeKind


def _store(sample_config):
    store = GraphStore()
    crawl(store, sample_config)
    return store


def test_extract_python_imports():
    node = Node(id="a.py", path="/x/a.py", kind=NodeKind.FILE)
    refs = references.extract_raw_references(node, "import b\nfrom c.d import e\n")
    assert "b" in refs
    assert "c.d" in refs


def test_extract_markdown_links():
    node = Node(id="docs/readme.md", path="/x/docs/readme.md", kind=NodeKind.FILE)
    refs = references.extract_raw_references(
        node, "see [guide](guide.md) and [entry](../a.py) and [web](https://x.com)\n"
    )
    assert "guide.md" in refs
    assert "../a.py" in refs
    assert "https://x.com" not in refs  # external URLs dropped


def test_resolve_python_import(sample_config):
    store = _store(sample_config)
    node = store.get_node("a.py")
    assert references.resolve_reference(store, node, "b") == "b.py"


def test_resolve_markdown_relative(sample_config):
    store = _store(sample_config)
    node = store.get_node("docs/readme.md")
    assert references.resolve_reference(store, node, "guide.md") == "docs/guide.md"
    assert references.resolve_reference(store, node, "../a.py") == "a.py"


def test_unresolved_reference_returns_none(sample_config):
    store = _store(sample_config)
    node = store.get_node("a.py")
    assert references.resolve_reference(store, node, "nonexistent_module") is None
