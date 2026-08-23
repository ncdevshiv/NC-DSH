"""Reflex Arc journal: append-only change history per fragment database.

Every filesystem-driven incremental re-crawl assembles a structured delta
report per changed path (the pinned wire contract shared with the MCP tool
layer) and appends it to the OWNING FRAGMENT's ``graph.db`` before emitting
``change`` on the bus. The journal is *history*: unlike nodes/edges/meta it is
never rebuilt from the live graph — saves carry existing rows forward verbatim
(see ``persist.save_sqlite``), so clients can page through changes with
``Core.call("changes", {"since_seq": ...})`` across re-crawls and restarts.

Delivery outbox: rows start with ``notified = 0``. After each re-crawl that
appended reports, the Core POSTs every un-notified report of the touched
fragment databases to the configured webhook URL and marks 2xx deliveries.
Failures stay un-notified and are retried on the next trigger (best-effort
v1 — see ``Core._flush_webhook``).
"""

from __future__ import annotations

import json
import sqlite3
from pathlib import Path

JOURNAL_SCHEMA = """
CREATE TABLE IF NOT EXISTS journal(
    seq INTEGER PRIMARY KEY AUTOINCREMENT,
    ts REAL NOT NULL,
    kind TEXT NOT NULL,
    path TEXT NOT NULL,
    root TEXT NOT NULL,
    old_hash TEXT DEFAULT '',
    new_hash TEXT DEFAULT '',
    report_json TEXT NOT NULL,
    notified INTEGER DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_journal_ts ON journal(ts);
"""


def ensure_journal(con: sqlite3.Connection) -> None:
    """Create the journal table + index if absent (idempotent)."""
    con.executescript(JOURNAL_SCHEMA)


def append_report(con: sqlite3.Connection, report: dict) -> int:
    """Append one change report; returns its journal ``seq`` (the rowid).

    The full report — including the authoritative ``seq`` — is stored as
    ``report_json``, so ``fetch_since`` round-trips the exact wire shape.
    """
    payload = dict(report)
    cur = con.execute(
        "INSERT INTO journal(ts, kind, path, root, old_hash, new_hash, report_json)"
        " VALUES (?, ?, ?, ?, ?, ?, ?)",
        (
            float(payload.get("ts", 0.0)),
            str(payload.get("kind", "")),
            str(payload.get("path", "")),
            str(payload.get("root", "")),
            str(payload.get("old_hash", "")),
            str(payload.get("new_hash", "")),
            "{}",
        ),
    )
    seq = int(cur.lastrowid)
    payload["seq"] = seq
    con.execute(
        "UPDATE journal SET report_json = ? WHERE seq = ?",
        (json.dumps(payload), seq),
    )
    con.commit()
    return seq


def fetch_since(con: sqlite3.Connection, since_seq: int = 0,
                limit: int = 200) -> list[dict]:
    """Parsed reports with ``seq > since_seq``, ascending by seq."""
    cur = con.execute(
        "SELECT seq, report_json FROM journal WHERE seq > ?"
        " ORDER BY seq ASC LIMIT ?",
        (int(since_seq), int(limit)),
    )
    out: list[dict] = []
    for seq, raw in cur.fetchall():
        try:
            report = json.loads(raw)
        except (TypeError, ValueError):
            continue  # corrupt row: skip rather than poison the replay
        if isinstance(report, dict):
            report.setdefault("seq", int(seq))
            out.append(report)
    return out


def mark_notified(con: sqlite3.Connection, seqs) -> None:
    """Flip ``notified`` to 1 for delivered reports (webhook outbox)."""
    ids = [int(s) for s in seqs]
    if not ids:
        return
    con.executemany(
        "UPDATE journal SET notified = 1 WHERE seq = ?",
        [(s,) for s in ids],
    )
    con.commit()


# ---- save-time carry-forward (used by persist.save_sqlite) -----------------
# save_sqlite builds a fresh DB and atomically replaces the old file. The
# journal is deliberately NOT cleared by saves, so the old rows are read
# before the replace and re-inserted into the fresh DB with their original
# seq values (explicit rowid inserts keep AUTOINCREMENT monotonic).

_JOURNAL_COLUMNS = (
    "seq, ts, kind, path, root, old_hash, new_hash, report_json, notified"
)


def load_rows(path: str | Path) -> list[tuple]:
    """Every journal row of an existing db, oldest first ([] on any error)."""
    p = Path(path)
    if not p.exists():
        return []
    try:
        con = sqlite3.connect(str(p))
        # Wait briefly on locks rather than failing while a save is in flight.
        con.execute("PRAGMA busy_timeout=5000")
        try:
            cur = con.execute(
                f"SELECT {_JOURNAL_COLUMNS} FROM journal ORDER BY seq ASC")
            return cur.fetchall()
        finally:
            con.close()
    except sqlite3.Error:
        return []  # no journal yet / unreadable: nothing to carry forward


def restore_rows(con: sqlite3.Connection, rows: list[tuple]) -> None:
    """Re-insert carried journal rows preserving their original seq values."""
    if not rows:
        return
    con.executemany(
        f"INSERT INTO journal({_JOURNAL_COLUMNS}) VALUES (?,?,?,?,?,?,?,?,?)",
        rows,
    )
