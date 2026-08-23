"""Reflex Arc: the worm notices, remembers, and explains filesystem changes.

Covers the journal round-trip (t1), per-change delta reports on the bus for
modified/deleted files (t2/t3), the ``changes`` replay op (t4), and the
best-effort webhook outbox (t5).

Note on event shapes: the pinned report carries its own ``kind``/``seq``
keys, which collide with ``EventBus.emit(kind, **payload)``'s positional
argument if splatted — so the Core broadcasts
``bus.emit("change", report=<pinned report>)`` and tests read the verbatim
report from the event's ``report`` key.
"""

from __future__ import annotations

import json
import sqlite3
import time
import urllib.request
from pathlib import Path

from dataworm.core import Core
from dataworm.journal import (
    append_report,
    ensure_journal,
    fetch_since,
    mark_notified,
)

# The pinned wire contract (shared with the MCP tool layer).
PINNED_KEYS = {
    "seq", "ts", "kind", "path", "root",
    "old_hash", "new_hash",
    "refs_lost", "refs_gained", "dangling_now",
    "dependents_before", "dependents_after",
    "source",
}


def _wait_for(predicate, timeout=8.0, interval=0.05):
    deadline = time.time() + timeout
    while time.time() < deadline:
        if predicate():
            return True
        time.sleep(interval)
    return False


def _is_change(event: dict) -> bool:
    """True when a captured bus event is one of our delta reports."""
    return event.get("kind") == "change" and isinstance(
        event.get("report"), dict)


def _reports(events: list) -> list[dict]:
    """The pinned reports of every change event captured so far."""
    return [e["report"] for e in events if _is_change(e)]


def _make_tree(root: Path) -> None:
    """a.py imports b.py; c.py is unreferenced bait for later edits."""
    (root / "a.py").write_text("import b\n\nprint('a')\n", encoding="utf-8")
    (root / "b.py").write_text("value = 1\n", encoding="utf-8")
    (root / "c.py").write_text("value = 2\n", encoding="utf-8")


def _crawled_core(tmp_path: Path) -> tuple[Core, Path]:
    root = tmp_path / "proj"
    root.mkdir()
    _make_tree(root)
    core = Core(db_path=str(tmp_path / "g.db"), prefer_rust=False)
    core.call("crawl", {"root": str(root), "max_cycles": 2,
                        "enable_semantic": False})
    return core, root


# ---- t1 --------------------------------------------------------------------

def test_journal_round_trip(tmp_path: Path) -> None:
    """append/fetch_since/mark_notified: ordering, filtering, cap, flags."""
    con = sqlite3.connect(":memory:")
    ensure_journal(con)
    base = {"ts": 1.0, "kind": "modified", "path": "b.py", "root": "/r",
            "old_hash": "aa", "new_hash": "bb", "refs_lost": [],
            "refs_gained": [], "dangling_now": [], "dependents_before": [],
            "dependents_after": [], "source": "fs_event"}
    s1 = append_report(con, dict(base))
    s2 = append_report(con, {**base, "ts": 2.0, "path": "a.py"})
    assert 1 <= s1 < s2

    rows = fetch_since(con, 0)
    assert [r["seq"] for r in rows] == [s1, s2]  # ascending by seq
    assert rows[0]["kind"] == "modified" and rows[1]["path"] == "a.py"
    for row in rows:
        assert set(row) == PINNED_KEYS  # full report round-trips via JSON

    # since_seq filtering.
    assert [r["seq"] for r in fetch_since(con, s1)] == [s2]
    assert fetch_since(con, s2) == []
    # Cap: at most `limit` reports, oldest first.
    assert [r["seq"] for r in fetch_since(con, 0, limit=1)] == [s1]

    mark_notified(con, [s1])
    flagged = con.execute(
        "SELECT seq FROM journal WHERE notified = 1").fetchall()
    assert [r[0] for r in flagged] == [s1]
    still_pending = con.execute(
        "SELECT seq FROM journal WHERE notified = 0").fetchall()
    assert [r[0] for r in still_pending] == [s2]


# ---- shared live-watch flow -------------------------------------------------

def _watch_and_change(core: Core, root: Path, events: list,
                      mutate) -> list[dict]:
    """Watch root, apply `mutate`, wait for the fs_event recrawl to settle."""
    core.bus.subscribe(events.append)
    core.call("watch", {"root": str(root), "poll_interval": 0.1})
    time.sleep(0.2)  # watcher spin-up (well under the debounce window)
    try:
        mutate()
        assert _wait_for(lambda: any(e.get("kind") == "done"
                                     and e.get("reason") == "fs_event"
                                     for e in events)), \
            f"no fs_event-driven recrawl seen in {events!r}"
        assert _wait_for(lambda: any(_is_change(e) for e in events)), \
            f"no change report seen in {events!r}"
        return _reports(events)
    finally:
        core.call("unwatch", {"root": str(root)})


# ---- t2 --------------------------------------------------------------------

def test_modified_file_produces_delta_report(tmp_path: Path) -> None:
    """Rewriting b.py emits ONE modified report with the full before/after."""
    core, root = _crawled_core(tmp_path)
    events: list[dict] = []

    def mutate() -> None:
        # Keep b.py importable; add a real reference so refs_gained is exact.
        (root / "b.py").write_text("import c\n\nvalue = 11\n", encoding="utf-8")

    changes = _watch_and_change(core, root, events, mutate)

    assert len(changes) == 1, [e.get("path") for e in changes]
    rep = changes[0]
    assert set(rep) >= PINNED_KEYS
    assert rep["kind"] == "modified"
    assert rep["path"] == "b.py"
    assert rep["source"] == "fs_event"
    assert rep["old_hash"] and rep["new_hash"]
    assert rep["old_hash"] != rep["new_hash"]
    assert rep["refs_gained"] == ["c.py"]
    assert rep["refs_lost"] == []
    assert rep["dangling_now"] == []
    assert rep["dependents_before"] == ["a.py"]
    assert rep["dependents_after"] == ["a.py"]
    assert isinstance(rep["seq"], int)
    assert isinstance(rep["ts"], float) and rep["ts"] > 0


# ---- t3 --------------------------------------------------------------------

def test_deleted_file_report(tmp_path: Path) -> None:
    """Deleting b.py emits ONE deleted report with empty new_hash."""
    core, root = _crawled_core(tmp_path)
    events: list[dict] = []
    changes = _watch_and_change(core, root, events,
                                lambda: (root / "b.py").unlink())

    assert len(changes) == 1, [e.get("path") for e in changes]
    rep = changes[0]
    assert rep["kind"] == "deleted"
    assert rep["path"] == "b.py"
    assert rep["new_hash"] == ""
    assert rep["dependents_before"] == ["a.py"]  # someone DID depend on it
    assert rep["source"] == "fs_event"


# ---- t4 --------------------------------------------------------------------

def test_changes_op_reads_journal(tmp_path: Path) -> None:
    """Core.call('changes') replays journalled reports with since_seq paging."""
    core, root = _crawled_core(tmp_path)
    events: list[dict] = []

    def mutate() -> None:
        (root / "b.py").write_text("import c\n\nvalue = 11\n", encoding="utf-8")

    _watch_and_change(core, root, events, mutate)

    out = core.call("changes", {"since_seq": 0})
    assert "error" not in out, out
    assert len(out["changes"]) >= 1
    # Manual crawls journal created-reports too (Trust & Foresight), so the
    # journal may open with "created" rows — select the fs-event modification.
    mod = [r for r in out["changes"]
           if r["kind"] == "modified" and r["path"] == "b.py"]
    assert mod, f"no modified b.py report in {out['changes']}"
    rep = mod[0]
    assert set(rep) >= PINNED_KEYS
    assert out["last_seq"] == max(r["seq"] for r in out["changes"])

    # since_seq filtering: everything already consumed → nothing new.
    out2 = core.call("changes", {"since_seq": out["last_seq"]})
    assert out2["changes"] == []

    # Root scoping: matching fragment returns it, unknown fragment doesn't.
    assert len(core.call("changes", {"since_seq": 0,
                                     "root": str(root)})["changes"]) >= 1
    other = tmp_path / "elsewhere"
    other.mkdir()
    assert core.call("changes", {"since_seq": 0,
                                 "root": str(other)})["changes"] == []


# ---- t5 --------------------------------------------------------------------

def test_webhook_posted(tmp_path: Path, monkeypatch) -> None:
    """configure_webhook + a real change POSTs the pinned report (no network)."""
    core, root = _crawled_core(tmp_path)

    posted: list[bytes] = []

    class _FakeResp:
        status = 200

        def __enter__(self):
            return self

        def __exit__(self, *exc):
            return False

    def _fake_urlopen(req, timeout=None):
        posted.append(req.data)  # urllib.request.Request body
        assert timeout == 5
        return _FakeResp()

    monkeypatch.setattr(urllib.request, "urlopen", _fake_urlopen)

    cfg = core.call("configure_webhook", {"url": "http://127.0.0.1:9/hook"})
    assert cfg == {"ok": True, "webhook_url": "http://127.0.0.1:9/hook"}

    events: list[dict] = []

    def mutate() -> None:
        # Give c.py a reference to b.py (b.py keeps importing nothing).
        (root / "c.py").write_text("import b\n\nvalue = 22\n", encoding="utf-8")

    _watch_and_change(core, root, events, mutate)

    assert len(posted) >= 1, "webhook was never called"
    body = json.loads(posted[-1])
    assert set(body) >= PINNED_KEYS
    assert body["kind"] == "modified" and body["path"] == "c.py"
    assert body["refs_gained"] == ["b.py"]
    assert body["source"] == "fs_event"

    # Delivered reports are marked notified in the owning fragment's journal.
    db = root / ".dataworm" / "graph.db"
    con = sqlite3.connect(str(db))
    try:
        rows = con.execute("SELECT notified FROM journal").fetchall()
    finally:
        con.close()
    assert rows, "journal rows missing from fragment db"
    assert all(r[0] == 1 for r in rows)
