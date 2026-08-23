"""Filesystem watcher: the worm's eyes.

Watches a directory tree for changes and emits ``fs_event`` events on the bus
so the Core can schedule a debounced incremental re-crawl. Two backends:

  1. ``watchdog`` (preferred) — real OS file notifications (inotify / ReadDirectoryChangesW /
     FSEvents / kqueue). Reactive in <1s. Installed via the ``watch`` extra.
  2. stdlib polling fallback — snapshots mtimes every ``poll_interval`` seconds.
     Zero-dependency default; works everywhere.

Both backends honour ``Config.ignore_dirs`` / ``ignore_globs`` so the worm never
reacts to noise it wouldn't crawl (``.git``, ``__pycache__``, ``.dataworm`` …).
"""

from __future__ import annotations

import logging
import os
import threading
import time
from pathlib import Path
from typing import Any, Callable

from dataworm.config import Config
from dataworm.crawler import _is_reparse_link
from dataworm.events import EventBus

log = logging.getLogger("dataworm.watcher")

# Event kinds emitted on the bus. Anything starting with ``fs_`` is a watcher
# signal, distinct from crawl-progress events (``node``/``edge``/``pass``/...).
EVENT_KIND_CREATED = "fs_created"
EVENT_KIND_MODIFIED = "fs_modified"
EVENT_KIND_DELETED = "fs_deleted"
EVENT_KIND_MOVED = "fs_moved"


class DirectoryWatcher:
    """Watch a single root directory tree and emit fs_event bus events.

    The watcher only *signals* changes; it does not re-crawl. The Core owns the
    debounced re-crawl scheduling (see ``Core._schedule_recrawl``) so the watcher
    stays a dumb, reliable emitter.

    Construction does not start watching; call ``start()``. ``stop()`` joins the
    background thread. Safe to call ``start()`` twice (idempotent).
    """

    def __init__(
        self,
        root: str | Path,
        bus: EventBus,
        config: Config | None = None,
        poll_interval: float = 1.5,
        debounce: float = 0.6,
        on_event: Callable[[str, str], None] | None = None,
    ) -> None:
        self.root = Path(root).resolve()
        self.bus = bus
        self.config = config or Config(root=str(self.root))
        self.poll_interval = poll_interval
        self.debounce = debounce
        self.on_event = on_event  # optional direct callback (tests)
        self._stop = threading.Event()
        self._thread: threading.Thread | None = None
        self._backend: str = ""
        self._observer: Any = None  # watchdog Observer if used

    @property
    def backend(self) -> str:
        return self._backend

    # ---- lifecycle -------------------------------------------------------

    def start(self) -> None:
        if self._thread is not None and self._thread.is_alive():
            return  # already running
        self._stop.clear()
        try:
            self._start_watchdog()
            self._backend = "watchdog"
        except Exception as exc:
            log.info("watchdog unavailable (%s); falling back to polling", exc)
            self._start_poller()
            self._backend = "polling"
        log.debug("watcher started on %s via %s", self.root, self._backend)

    def stop(self) -> None:
        self._stop.set()
        if self._observer is not None:
            try:
                self._observer.stop()
                self._observer.join(timeout=2.0)
            except Exception:
                pass
            self._observer = None
        if self._thread is not None and self._thread is not threading.current_thread():
            self._thread.join(timeout=2.0)
        self._thread = None

    # ---- watchdog backend ----------------------------------------------

    def _start_watchdog(self) -> None:
        from watchdog.observers import Observer  # type: ignore
        from watchdog.events import FileSystemEventHandler  # type: ignore

        watcher = self  # closure handle

        class _Handler(FileSystemEventHandler):
            def _emit(self, kind: str, src_path: str) -> None:
                if watcher._ignored(src_path):
                    return
                watcher._emit(kind, src_path)

            def on_created(self, event: Any) -> None:
                if not event.is_directory:
                    self._emit(EVENT_KIND_CREATED, event.src_path)

            def on_modified(self, event: Any) -> None:
                if not event.is_directory:
                    self._emit(EVENT_KIND_MODIFIED, event.src_path)

            def on_deleted(self, event: Any) -> None:
                if not event.is_directory:
                    self._emit(EVENT_KIND_DELETED, event.src_path)

            def on_moved(self, event: Any) -> None:
                if not event.is_directory:
                    self._emit(EVENT_KIND_MOVED, getattr(event, "dest_path", event.src_path))

        self._observer = Observer()
        self._observer.schedule(_Handler(), str(self.root), recursive=True)
        self._observer.start()

    # ---- polling backend ----------------------------------------------

    def _start_poller(self) -> None:
        # Capture the baseline snapshot synchronously: if the thread took it
        # lazily, a mutation landing between start() and the thread's first
        # snapshot would be baked into the baseline and never reported.
        baseline = self._snapshot()
        self._thread = threading.Thread(
            target=self._poll_loop, args=(baseline,), daemon=True,
        )
        self._thread.start()

    def _poll_loop(self, baseline: dict[str, float]) -> None:
        last = baseline
        while not self._stop.is_set():
            self._stop.wait(self.poll_interval)
            if self._stop.is_set():
                break
            now = self._snapshot()
            for path, mtime in now.items():
                prev = last.get(path)
                if prev is None:
                    self._emit_if_not_ignored(EVENT_KIND_CREATED, path)
                elif prev != mtime:
                    self._emit_if_not_ignored(EVENT_KIND_MODIFIED, path)
            for path in last.keys() - now.keys():
                self._emit_if_not_ignored(EVENT_KIND_DELETED, path)
            last = now

    def _snapshot(self) -> dict[str, float]:
        """Map of file path -> mtime for every non-ignored file under root.

        Walks iteratively via ``os.scandir`` and NEVER descends into reparse
        points (symlink/junction dirs, gated by ``crawler._is_reparse_link``).
        On Windows junctions are not ``is_symlink()``, so the previous
        ``self.root.rglob("*")`` descended into them — and a junction CYCLE
        hung this poller thread forever. Reparse entries themselves are
        invisible to the snapshot: never descended into, never stat'd through.
        Per-entry OSErrors are swallowed exactly like before.
        """
        snap: dict[str, float] = {}
        stack = [self.root]
        while stack:
            current = stack.pop()
            try:
                entries = sorted(os.scandir(current), key=lambda e: e.name)
            except OSError:
                continue  # unreadable dir: skip it entirely
            for entry in entries:
                sp = str(entry.path)
                if _is_reparse_link(Path(sp)):
                    # Junction/symlink (dirs AND files): invisible to the
                    # snapshot — cycle-safe by construction.
                    continue
                if self._ignored(sp):
                    continue
                try:
                    if entry.is_dir(follow_symlinks=False):
                        # Real dir: scan it on a later stack pop.
                        stack.append(Path(sp))
                        continue
                    snap[sp] = entry.stat(follow_symlinks=False).st_mtime
                except OSError:
                    continue
        return snap

    # ---- shared helpers -------------------------------------------------

    def _ignored(self, path: str) -> bool:
        """Honour the crawl's ignore rules so noise never fires a re-crawl."""
        try:
            p = Path(path)
        except Exception:
            return True
        parts = self._rel_parts(p)
        if parts is None:
            # Outside the root (or unresolvable): fall back to raw parts so a
            # path-shaped ignore rule can still match.
            parts = p.parts
        for part in parts:
            if self.config.should_ignore_dir(part):
                return True
        rel_id = "/".join(parts) if parts is not None else p.name
        return self.config.should_ignore_file(rel_id, p.name)

    def _rel_parts(self, p: Path) -> tuple[str, ...] | None:
        """Parts of ``p`` relative to root, or None when outside it.

        Both sides are resolved first: on Windows an 8.3 short path
        (``C:\\Users\\NCDEVS~1\\...``) is a *different string* than its long
        form, and `relative_to` compares component-wise — without resolve(),
        such paths would crash this method (killing the poller thread).
        """
        try:
            return tuple(p.resolve().relative_to(self.root).parts)
        except (ValueError, OSError):
            return None

    def _emit_if_not_ignored(self, kind: str, path: str) -> None:
        if self._ignored(path):
            return
        self._emit(kind, path)

    def _emit(self, kind: str, path: str) -> None:
        if self.on_event is not None:
            try:
                self.on_event(kind, path)
            except Exception:
                log.exception("on_event callback failed")
        # Always emit on the bus so the Core sees it.
        self.bus.emit(kind, path=path, root=str(self.root))


# ---- helper for callers (Core) -------------------------------------------

def create_watcher(
    root: str | Path,
    bus: EventBus,
    config: Config | None = None,
    poll_interval: float = 1.5,
    debounce: float = 0.6,
    on_event: Callable[[str, str], None] | None = None,
) -> DirectoryWatcher:
    """Construct a DirectoryWatcher. Does not start it (call .start())."""
    return DirectoryWatcher(
        root=root, bus=bus, config=config,
        poll_interval=poll_interval, debounce=debounce, on_event=on_event,
    )
