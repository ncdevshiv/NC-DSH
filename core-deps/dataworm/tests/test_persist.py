from dataworm.persist import load_json, load_sqlite, save_json, save_sqlite


def test_sqlite_roundtrip(sample_store, tmp_path):
    db = tmp_path / "graph.db"
    save_sqlite(sample_store, db)
    loaded = load_sqlite(db)

    assert loaded.counts() == sample_store.counts()
    assert loaded.root == sample_store.root
    assert loaded.meta.get("converged") == sample_store.meta.get("converged")
    # Spot-check a node and an edge survived the trip.
    assert loaded.get_node("a.py") is not None
    from dataworm.models import EdgeType
    assert loaded.get_edge("a.py", "b.py", EdgeType.REFERENCES) is not None


def test_json_roundtrip(sample_store, tmp_path):
    out = tmp_path / "graph.json"
    save_json(sample_store, out)
    loaded = load_json(out)
    assert loaded.counts() == sample_store.counts()
    assert loaded.get_node("docs/readme.md") is not None


def test_save_sqlite_retries_replace_when_target_locked(sample_store, tmp_path,
                                                        monkeypatch):
    """Windows: another connection can transiently hold the target open when
    the atomic rename fires; save_sqlite must retry (3 attempts) instead of
    losing the whole save."""
    import os as _os

    db = tmp_path / "graph.db"
    db.write_bytes(b"stale-bytes")  # an existing target file
    calls = {"n": 0}
    real_replace = _os.replace

    def flaky_replace(src, dst):
        calls["n"] += 1
        if calls["n"] < 3:
            raise PermissionError(13, "The process cannot access the file")
        return real_replace(src, dst)

    monkeypatch.setattr("dataworm.persist.os.replace", flaky_replace)
    save_sqlite(sample_store, db)
    assert calls["n"] == 3  # failed twice, succeeded on the third attempt
    # The final write is the real graph (atomicity preserved end-to-end).
    assert load_sqlite(db).counts() == sample_store.counts()
