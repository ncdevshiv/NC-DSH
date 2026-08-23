from dataworm.query import QueryAPI


def _api(sample_store):
    return QueryAPI(sample_store)


def test_impact_of_leaf_has_direct_and_transitive(sample_store):
    api = _api(sample_store)
    result = api.impact_of("c.py")
    # b.py references c.py directly; a.py references b.py -> transitive.
    assert "b.py" in result["direct"]
    assert "a.py" in result["transitive"]
    assert result["total_affected"] >= 2


def test_impact_of_unknown_path(sample_store):
    api = _api(sample_store)
    result = api.impact_of("does/not/exist.py")
    assert "error" in result


def test_context_for_includes_links_and_impact(sample_store):
    api = _api(sample_store)
    ctx = api.context_for("b.py")
    assert ctx["node"]["id"] == "b.py"
    assert "references" in ctx["link_counts"]
    assert ctx["impact"]["target"] == "b.py"


def test_neighbors_within_depth(sample_store):
    api = _api(sample_store)
    result = api.neighbors("b.py", depth=1)
    ids = {n["id"] for n in result["neighbors"]}
    assert "a.py" in ids  # a references b
    assert "c.py" in ids  # b references c


def test_search_finds_by_substring(sample_store):
    api = _api(sample_store)
    hits = api.search("helper")
    assert any(h["id"] == "utils/helper.py" for h in hits)


def test_summary_counts(sample_store):
    api = _api(sample_store)
    summary = api.summary()
    assert summary["node_kinds"]["file"] > 0
    assert summary["node_kinds"]["dir"] > 0
    assert summary["edges_contains"] > 0


def test_to_id_accepts_absolute_path(sample_store):
    api = _api(sample_store)
    node = sample_store.get_node("a.py")
    assert api.to_id(node.path) == "a.py"
