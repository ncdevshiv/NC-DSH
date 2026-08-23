"""Stage 1.2: filesystem watcher emits fs_event and triggers a debounced re-crawl.

Uses the stdlib polling backend by default (no watchdog dependency required for
the test), so it runs everywhere.
"""

from __future__ import annotations

import time
from pathlib import Path

import pytest

from dataworm.config import Config
from dataworm.core import Core
from dataworm.events import EventBus
from dataworm.watcher import DirectoryWatcher


def _wait_for(predicate, timeout=5.0, interval=0.05):
    deadline = time.time() + timeout
    while time.time() < deadline:
        if predicate():
            return True
        time.sleep(interval)
    return False


def test_watcher_emits_on_file_create(tmp_path: Path) -> None:
    """Creating a file under the watched root fires an fs_created event."""
    bus = EventBus()
    seen: list[dict] = []
    bus.subscribe(lambda ev: seen.append(ev))

    root = tmp_path / "proj"
    root.mkdir()
    watcher = DirectoryWatcher(root, bus, Config(root=str(root)), poll_interval=0.1)
    watcher.start()
    try:
        assert watcher.backend in ("watchdog", "polling")
        (root / "new.py").write_text("print('hi')\n", encoding="utf-8")
        assert _wait_for(lambda: any(e["kind"] == "fs_created" for e in seen)), \
            f"no fs_created seen in {seen}"
    finally:
        watcher.stop()


def test_watcher_ignores_noise(tmp_path: Path) -> None:
    """A file in an ignored dir (e.g. __pycache__) must not fire an event."""
    bus = EventBus()
    seen: list[dict] = []
    bus.subscribe(lambda ev: seen.append(ev))

    root = tmp_path / "proj"
    (root / "__pycache__").mkdir(parents=True)
    watcher = DirectoryWatcher(root, bus, Config(root=str(root)), poll_interval=0.1)
    watcher.start()
    try:
        (root / "__pycache__" / "junk.pyc").write_bytes(b"\x00")
        time.sleep(0.5)  # let the poller cycle
    finally:
        watcher.stop()
    assert not any("__pycache__" in e.get("path", "") for e in seen), seen


def test_core_watch_triggers_incremental_recrawl(tmp_path: Path) -> None:
    """A watched Core re-crawls when a file changes, emitting a fresh done event."""
    db = str(tmp_path / "g.db")
    core = Core(db_path=db, prefer_rust=False)  # python path: deterministic in tests

    root = tmp_path / "proj"
    root.mkdir()
    (root / "a.py").write_text("print('a')\n", encoding="utf-8")
    core.call("crawl", {"root": str(root), "max_cycles": 2, "enable_semantic": False})

    done_events: list[dict] = []
    core.bus.subscribe(lambda ev: done_events.append(ev))

    # Start watching with a fast poll interval.
    core.call("watch", {"root": str(root), "poll_interval": 0.1})

    # Mutate a file: this should trigger a debounced incremental re-crawl,
    # which emits a `done` event with reason=fs_event.
    (root / "a.py").write_text("print('changed')\n", encoding="utf-8")

    try:
        ok = _wait_for(
            lambda: any(e.get("kind") == "done" and e.get("reason") == "fs_event"
                        for e in done_events),
            timeout=8.0,
        )
        assert ok, f"no fs_event-driven done seen in {done_events}"
    finally:
        core.call("unwatch", {"root": str(root)})


def test_watch_is_idempotent(tmp_path: Path) -> None:
    """Watching the same root twice does not spawn two watchers."""
    db = str(tmp_path / "g.db")
    core = Core(db_path=db, prefer_rust=False)
    root = tmp_path / "proj"
    root.mkdir()
    r1 = core.call("watch", {"root": str(root), "poll_interval": 0.1})
    r2 = core.call("watch", {"root": str(root), "poll_interval": 0.1})
    assert r1["status"] == "watching"
    assert r2["status"] == "already watching"
    assert len(core._watchers) == 1
    core.call("unwatch", {"root": str(root)})
