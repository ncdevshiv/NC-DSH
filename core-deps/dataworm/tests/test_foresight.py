"""Trust & Foresight: what-if edit simulation (plan_edit).

All fixtures write EXACT byte content (write_text adds no trailing newline),
so hash-equality assertions are deterministic.
"""

from __future__ import annotations

import pytest

from dataworm.core import Core


@pytest.fixture
def core(tmp_path):
    c = Core(db_path=str(tmp_path / ".dataworm" / "graph.db"), prefer_rust=False)
    return c


@pytest.fixture
def tree(tmp_path):
    root = tmp_path / "proj"
    root.mkdir()
    (root / "a.py").write_text("import b", encoding="utf-8", newline="")
    (root / "b.py").write_text("val = 1", encoding="utf-8", newline="")
    (root / "dup.py").write_text("import b", encoding="utf-8", newline="")
    (root / "c.py").write_text("def helper(): pass", encoding="utf-8", newline="")
    return root


def crawl(core, root):
    core.call("crawl", {"root": str(root)})


# ---- plan_edit ---------------------------------------------------------------

def test_plan_edit_gained_link_and_md_dangling(core, tree):
    crawl(core, tree)
    # a.py gains a real link to c.py.
    r = core.call("plan_edit", {
        "path": "a.py",
        "content": "import b\nimport c",
    })
    assert r["refs_gained"] == ["c.py"]
    assert r["refs_lost"] == []
    assert r["unchanged"] is False
    # Dangling detection rides the extractor per language: quoted relative
    # paths are extracted from MARKDOWN (not .py), so probe that surface.
    r_md = core.call("plan_edit", {
        "path": "notes.md",
        "content": "see [x](./missing.txt)",
    })
    assert "./missing.txt" in r_md["dangling_now"]


def test_plan_edit_lost_link(core, tree):
    crawl(core, tree)
    r = core.call("plan_edit", {"path": "a.py", "content": "x = 1"})
    assert r["refs_lost"] == ["b.py"]
    assert r["refs_gained"] == []


def test_plan_edit_unchanged_short_circuit(core, tree):
    crawl(core, tree)
    r = core.call("plan_edit", {"path": "a.py", "content": "import b"})
    assert r["unchanged"] is True
    assert r["current_hash"] == r["new_hash"]
    assert r["refs_gained"] == [] and r["refs_lost"] == []
    assert r["near_duplicates"] == [] and r["exact_duplicate_of"] == ""


def test_plan_edit_exact_duplicate_radar(core, tree):
    crawl(core, tree)
    r = core.call("plan_edit", {"path": "a.py", "content": "val = 1"})
    assert r["exact_duplicate_of"] == "b.py"


def test_plan_edit_near_duplicate_radar(core, tree):
    crawl(core, tree)
    # Simhash fingerprints TOKEN MULTISETS: whitespace-only byte changes keep
    # the fingerprint identical (hamming 0) while bytes differ — so this is
    # near-dup WITHOUT being an exact sha256 twin. Fully deterministic.
    r = core.call("plan_edit", {"path": "a.py", "content": "import b  "})
    entries = {d["id"]: d["hamming"] for d in r["near_duplicates"]}
    assert "dup.py" in entries and "b.py" not in entries
    assert all(h <= 3 for h in entries.values())
    assert r["exact_duplicate_of"] == ""  # bytes differ -> not exact
    # Its own old fingerprint must never be reported as its own twin.
    assert "a.py" not in entries


def test_plan_edit_brand_new_file(core, tree):
    crawl(core, tree)
    r = core.call("plan_edit", {"path": "new.py", "content": "import a"})
    assert r["current_hash"] == ""
    assert r["unchanged"] is False
    assert r["refs_gained"] == ["a.py"]
    assert r["dependents_count"] == 0


def test_plan_edit_requires_content(core, tree):
    crawl(core, tree)
    r = core.call("plan_edit", {"path": "a.py"})
    assert "error" in r


# ---- manual-crawl journaling (Reflex Arc blind-spot fix) --------------------

def test_manual_crawl_journals_modified(tmp_path, core):
    root = tmp_path / "m"
    root.mkdir()
    (root / "a.py").write_text("import b", encoding="utf-8", newline="")
    (root / "b.py").write_text("val = 1", encoding="utf-8", newline="")
    crawl(core, root)

    seen = []
    core.bus.subscribe(lambda ev: seen.append(ev)
                       if ev.get("kind") == "change" else None)
    (root / "b.py").write_text("val = 2\ndef extra(): pass", encoding="utf-8",
                               newline="")
    r = core.call("crawl", {"root": str(root)})  # MANUAL — no watcher involved

    assert r["converged"] is True
    reports = [ev["report"] for ev in seen]
    mod = [x for x in reports if x["kind"] == "modified" and x["path"] == "b.py"]
    assert mod, f"no modified report in {reports}"
    rep = mod[-1]
    assert rep["source"] == "manual_crawl"
    assert rep["old_hash"] and rep["new_hash"] and rep["old_hash"] != rep["new_hash"]
    # Redundant immediate re-crawl appends nothing (nothing changed).
    before = len(seen)
    core.call("crawl", {"root": str(root)})
    assert len(seen) == before


def test_manual_crawl_journals_deleted(tmp_path, core):
    root = tmp_path / "m"
    root.mkdir()
    (root / "a.py").write_text("import b", encoding="utf-8", newline="")
    (root / "b.py").write_text("val = 1", encoding="utf-8", newline="")
    crawl(core, root)

    seen = []
    core.bus.subscribe(lambda ev: seen.append(ev)
                       if ev.get("kind") == "change" else None)
    (root / "b.py").unlink()
    core.call("crawl", {"root": str(root)})

    reports = [ev["report"] for ev in seen]
    dele = [x for x in reports if x["kind"] == "deleted" and x["path"] == "b.py"]
    assert dele, f"no deleted report in {reports}"
    assert dele[-1]["new_hash"] == ""
    assert dele[-1]["old_hash"]
    assert "a.py" in dele[-1]["dependents_before"]


def test_first_crawl_reports_created_burst_capped(tmp_path, core):
    root = tmp_path / "m"
    root.mkdir()
    for i in range(60):
        (root / f"f{i:02d}.py").write_text(f"x{i} = {i}", encoding="utf-8",
                                           newline="")
    seen = []
    core.bus.subscribe(lambda ev: seen.append(ev)
                       if ev.get("kind") == "change" else None)
    crawl(core, root)
    kinds = [ev["report"]["kind"] for ev in seen]
    assert kinds.count("created") == 50          # individual cap
    bursts = [ev["report"] for ev in seen if ev["report"]["kind"] == "burst"]
    assert len(bursts) == 1 and len(bursts[0]["paths"]) == 10
