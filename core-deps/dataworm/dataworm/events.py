"""Real-time event bus.

The graph emits an event on every *actual* mutation (node added, edge added,
dimension recomputed) and the engine emits lifecycle events (pass/cycle/done).
Nothing here is simulated — subscribers observe the genuine build as it happens.

Each event carries a monotonically increasing ``seq`` so a client that replays a
buffered history and then switches to the live stream can skip duplicates.
"""

from __future__ import annotations

import itertools
import threading
from typing import Any, Callable

Subscriber = Callable[[dict[str, Any]], None]


class EventBus:
    def __init__(self) -> None:
        self._subscribers: list[Subscriber] = []
        self._seq = itertools.count(1)
        self._lock = threading.Lock()

    def subscribe(self, fn: Subscriber) -> None:
        with self._lock:
            self._subscribers.append(fn)

    def unsubscribe(self, fn: Subscriber) -> None:
        with self._lock:
            if fn in self._subscribers:
                self._subscribers.remove(fn)

    def emit(self, kind: str, **payload: Any) -> dict[str, Any]:
        """Broadcast an event to all subscribers. Returns the event dict."""
        event = {"seq": next(self._seq), "kind": kind, **payload}
        with self._lock:
            subscribers = list(self._subscribers)
        for fn in subscribers:
            try:
                fn(event)
            except Exception:
                # A misbehaving subscriber must not break the crawl.
                pass
        return event


class NodeEventBatcher:
    """Coalesce per-node ``node`` events into ``nodes_batch`` events.

    During a structural ingest of a million-file tree, emitting one ``node``
    event per file floods the bus (and the SSE queue) with 1M individual
    events. The batcher buffers them and flushes a single ``nodes_batch`` event
    every ``batch_size`` nodes (default 200) — so the browser processes ~5k
    batches instead of 1M events, and the per-event ``countEdges`` cost (which
    was O((N+E)^2) over a crawl) disappears.

    Lifecycle events (``pass``/``cycle``/``done``/``edge``/``reset_dim``) pass
    through untouched — only ``node`` events are batched. Call ``flush()`` at
    the end of the ingest to emit any remainder.
    """

    def __init__(self, bus: EventBus, batch_size: int = 200) -> None:
        self.bus = bus
        self.batch_size = batch_size
        self._buffer: list[dict[str, Any]] = []
        self._total = 0  # total nodes seen, for progress reporting

    def add(self, node_id: str, node_kind: str, path: str, size: int) -> None:
        """Buffer a node event; flush automatically when the buffer is full."""
        self._buffer.append({"id": node_id, "node_kind": node_kind, "path": path, "size": size})
        self._total += 1
        # Emit a progress event every 1000 nodes so the dashboard can show
        # "discovered N nodes" during a long structural ingest.
        if self._total % 1000 == 0:
            self.bus.emit("progress", discovered=self._total)
        if len(self._buffer) >= self.batch_size:
            self.flush()

    def flush(self) -> None:
        """Emit any buffered nodes as a single ``nodes_batch`` event."""
        if not self._buffer:
            return
        batch = self._buffer
        self._buffer = []
        self.bus.emit("nodes_batch", nodes=batch, count=len(batch))
