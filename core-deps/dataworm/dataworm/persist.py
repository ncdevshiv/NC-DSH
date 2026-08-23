"""Persistence: save/load the graph to SQLite (queryable) and JSON (portable)."""

from __future__ import annotations

import json
import os
import sqlite3
import time
from pathlib import Path

from dataworm.graph import GraphStore
from dataworm.journal import ensure_journal, load_rows as load_journal_rows
from dataworm.journal import restore_rows as restore_journal_rows
from dataworm.models import Edge, Node

_SCHEMA = """
CREATE TABLE IF NOT EXISTS nodes (
    id           TEXT PRIMARY KEY,
    path         TEXT,
    kind         TEXT,
    size         INTEGER,
    mtime        REAL,
    content_hash TEXT,
    mime         TEXT,
    root         TEXT,
    attrs_json   TEXT
);
CREATE TABLE IF NOT EXISTS edges (
    src        TEXT,
    dst        TEXT,
    type       TEXT,
    weight     REAL,
    attrs_json TEXT,
    PRIMARY KEY (src, dst, type)
);
CREATE TABLE IF NOT EXISTS meta (
    key   TEXT PRIMARY KEY,
    value TEXT
);
-- Content-addressed memo (dirty-set recomputation): pass outputs keyed by
-- sha256 content_hash so reference extraction, simhash fingerprints, and
-- embeddings survive process restarts. `kind` is one of "refs" / "simhash" /
-- "embed"; value_json is the memoized output.
CREATE TABLE IF NOT EXISTS memo (
    kind       TEXT,
    key        TEXT,
    value_json TEXT,
    PRIMARY KEY (kind, key)
);
CREATE INDEX IF NOT EXISTS idx_edges_dst ON edges(dst, type);
CREATE INDEX IF NOT EXISTS idx_edges_src ON edges(src, type);
"""

# Memo kinds + the per-kind entry cap. Choice (documented per the memo design):
# cap PER KIND at save AND at load, keeping the NEWEST entries by insertion
# order — Python dicts preserve insertion order and memo entries are inserted
# as files are discovered, so "last N" == "most recently crawled". A bound at
# both write and read keeps DB size and load time O(cap) even for huge trees;
# evicted hashes simply fall back to cold recomputation.
MEMO_KINDS = ("refs", "simhash", "embed")
MEMO_MAX_ENTRIES_PER_KIND = 50_000

# Old DBs (pre-multi-root) lack the `root` column on nodes. Add it lazily so
# existing .dataworm/graph.db files keep loading after upgrade.
_MIGRATIONS = [
    ("nodes_root_col", "ALTER TABLE nodes ADD COLUMN root TEXT DEFAULT ''"),
]


def _ensure_schema(con: sqlite3.Connection) -> None:
    con.executescript(_SCHEMA)
    # The Reflex Arc journal (append-only change history) lives in the same
    # per-fragment db. Created lazily here so every save/load path has it.
    ensure_journal(con)
    cur = con.execute("PRAGMA table_info(nodes)")
    cols = {row[1] for row in cur.fetchall()}
    if "root" not in cols:
        try:
            con.execute("ALTER TABLE nodes ADD COLUMN root TEXT DEFAULT ''")
        except sqlite3.OperationalError:
            pass  # already exists


def _empty_memo() -> dict[str, dict]:
    return {"refs": {}, "simhash": {}, "embed": {}}


def _cap_memo(memo: dict[str, dict]) -> dict[str, dict]:
    """Keep the newest MEMO_MAX_ENTRIES_PER_KIND entries per kind."""
    capped: dict[str, dict] = {}
    for kind in MEMO_KINDS:
        entries = memo.get(kind) or {}
        if len(entries) > MEMO_MAX_ENTRIES_PER_KIND:
            entries = dict(list(entries.items())[-MEMO_MAX_ENTRIES_PER_KIND:])
        capped[kind] = entries
    return capped


def save_memo(con: sqlite3.Connection, memo: dict[str, dict] | None) -> None:
    """Write the content-addressed memo into the ``memo`` table.

    Keys are content hashes (refs keys carry the file extension too, since raw
    reference extraction is extension-dependent). Embed vectors are sparse
    ``{term_index: weight}`` dicts serialized as JSON objects.
    """
    con.executemany(
        "INSERT INTO memo VALUES (?,?,?)",
        [
            (kind, key, json.dumps(value))
            for kind, entries in _cap_memo(memo or {}).items()
            for key, value in entries.items()
        ],
    )


def load_memo(con: sqlite3.Connection) -> dict[str, dict]:
    """Read the content-addressed memo back. Corrupt/absent rows are skipped
    (a missing entry only costs a cold recompute, never a wrong result)."""
    memo = _empty_memo()
    try:
        cur = con.execute("SELECT kind, key, value_json FROM memo")
    except sqlite3.OperationalError:
        return memo  # pre-memo DB
    for kind, key, raw in cur.fetchall():
        if kind not in memo:
            continue
        try:
            value = json.loads(raw)
        except (TypeError, ValueError):
            continue
        if kind == "embed" and isinstance(value, dict):
            # JSON object keys are strings; vectors are {int_index: weight}.
            value = {
                int(k): float(v) for k, v in value.items()
                if isinstance(v, (int, float))
            }
        memo[kind][key] = value
    return _cap_memo(memo)


def save_sqlite(store: GraphStore, path: str | Path) -> None:
    """Persist the store atomically.

    Writes to a temp file then ``os.replace`` (atomic rename) so a crash
    mid-save (OOM, timeout-kill, Ctrl-C) leaves the *previous* graph intact
    rather than deleting it. Uses WAL journal mode for faster large writes.
    For multi-root merged stores, the `roots` set is stored in meta as JSON so
    it round-trips; each node also carries its own `root` provenance column.
    """
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_suffix(path.suffix + ".tmp")
    # The journal is append-only HISTORY, deliberately not cleared by saves:
    # carry its rows from the existing db into the fresh one (same seq values,
    # so AUTOINCREMENT stays monotonic and `changes(since_seq=...)` paging is
    # stable across re-saves). No-op when the old db has no journal yet.
    carried_journal = load_journal_rows(path)
    con = sqlite3.connect(tmp)
    # Wait (instead of failing) when another connection holds the db briefly.
    con.execute("PRAGMA busy_timeout=5000")
    try:
        con.execute("PRAGMA journal_mode=WAL")
        _ensure_schema(con)
        con.executemany(
            "INSERT INTO nodes VALUES (?,?,?,?,?,?,?,?,?)",
            [
                (n.id, n.path, n.kind.value, n.size, n.mtime,
                 n.content_hash, n.mime, n.root, json.dumps(n.attrs))
                for n in store.all_nodes()
            ],
        )
        con.executemany(
            "INSERT INTO edges VALUES (?,?,?,?,?)",
            [
                (e.src, e.dst, e.type.value, e.weight, json.dumps(e.attrs))
                for e in store.all_edges()
            ],
        )
        meta = dict(store.meta)
        meta["roots"] = sorted(store.roots)  # preserve the multi-root set
        con.executemany(
            "INSERT INTO meta VALUES (?,?)",
            [(k, json.dumps(v)) for k, v in meta.items()],
        )
        save_memo(con, getattr(store, "memo", None))
        restore_journal_rows(con, carried_journal)
        con.commit()
    finally:
        con.close()
    # Atomic rename: the old DB stays intact until the new one is fully written.
    # Windows quirk: another connection (a concurrent reader/loader) can hold
    # the target file open for a moment — retry a bounded few times before
    # giving up. Atomicity is untouched: os.replace only ever swaps a fully
    # written tmp over the previous db, and a failed attempt leaves both files.
    for attempt in range(3):
        try:
            os.replace(tmp, path)
            return
        except PermissionError:
            if attempt == 2:
                raise
            time.sleep(0.2)


def load_sqlite(path: str | Path) -> GraphStore:
    con = sqlite3.connect(path)
    # Wait briefly on locks rather than erroring when another process reads.
    con.execute("PRAGMA busy_timeout=5000")
    try:
        _ensure_schema(con)
        con.row_factory = sqlite3.Row
        cur = con.execute("PRAGMA table_info(nodes)")
        cols = {row[1] for row in cur.fetchall()}
        has_root_col = "root" in cols
        store = GraphStore()
        for row in con.execute("SELECT * FROM nodes"):
            store.add_node(Node(
                id=row["id"], path=row["path"], kind=_kind(row["kind"]),
                size=row["size"], mtime=row["mtime"],
                content_hash=row["content_hash"], mime=row["mime"],
                root=(row["root"] if has_root_col else ""),
                attrs=json.loads(row["attrs_json"] or "{}"),
            ))
        for row in con.execute("SELECT * FROM edges"):
            store.add_edge(Edge(
                src=row["src"], dst=row["dst"], type=_etype(row["type"]),
                weight=row["weight"], attrs=json.loads(row["attrs_json"] or "{}"),
            ))
        for row in con.execute("SELECT * FROM meta"):
            store.meta[row["key"]] = json.loads(row["value"])
        store.root = store.meta.get("root", "")
        # Restore the multi-root set if present (multi-root merged stores).
        roots_meta = store.meta.get("roots")
        if isinstance(roots_meta, list):
            store.roots = set(roots_meta)
        elif store.root:
            store.roots = {store.root}
        # Restore the content-addressed memo (refs/simhash/embed by hash).
        store.memo.update(load_memo(con))
        return store
    finally:
        con.close()


def save_json(store: GraphStore, path: str | Path) -> None:
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    payload = {
        "root": store.root,
        "meta": store.meta,
        "nodes": [n.to_dict() for n in store.all_nodes()],
        "edges": [e.to_dict() for e in store.all_edges()],
    }
    path.write_text(json.dumps(payload, indent=2), encoding="utf-8")


def load_json(path: str | Path) -> GraphStore:
    payload = json.loads(Path(path).read_text(encoding="utf-8"))
    store = GraphStore(root=payload.get("root", ""))
    store.meta = payload.get("meta", {})
    for nd in payload.get("nodes", []):
        store.add_node(Node.from_dict(nd))
    for ed in payload.get("edges", []):
        store.add_edge(Edge.from_dict(ed))
    return store


def _kind(value: str):
    from dataworm.models import NodeKind
    return NodeKind(value)


def _etype(value: str):
    from dataworm.models import EdgeType
    return EdgeType(value)
