# DataWorm Architecture

Deep dive for contributors; every section cites its implementing files. Companion overview: [README](../README.md).

## The convergence loop

```
            ┌────────────────────────────── one cycle ─────────────────────────────┐
            ▼                                                                     │
      crawl_pass          reference_pass       hashing_pass       semantic_pass     │
   nodes + contains  ──►  references edges ──►  duplicate_of  ──►  similar_to       │
   (structure)            (imports/links)       (sha256+simhash)   (embeddings)     │
            │                                                                     │
            └────────────────► signature() ◄───────────────────────────────────────┘
                                    │
                    signature == previous cycle? ── yes ──► converged
                                    │ no
                                    └── next cycle, or stop at max_cycles
```

`engine.run()` (`dataworm/engine.py`, `run()` at L427) loops until the store's
signature is stable or `max_cycles` is hit; a later cycle resolves references
that dangled in an earlier one, which is what lets the loop reach a fixed point.

## Dimensions

| Edge type | Built by | Meaning | Notes |
|-----------|----------|---------|-------|
| `contains` | crawl pass | directory → child dir/file | rebuilt structurally each crawl |
| `references` | reference pass | file imports/links file | unresolved targets tracked as dangling; cross-dir links marked `cross_dir` |
| `duplicate_of` | hashing pass | identical/near-identical content | exact: weight 1.0 (`reason: "exact"`, sha256); near: weight 0.9 (`reason: "near"`, hamming) |
| `similar_to` | semantic pass | cosine ≥ `similarity_threshold` (default 0.35) | pluggable embedder; TF-IDF default |

Edge model in `dataworm/models.py`; passes in `dataworm/engine.py` and their
Rust twins in `rust/src/pass.rs`.

## Engine / passes flow

One JSON contract — `{method, params} → {result | error}` — executed only by
`Core.call()` (`dataworm/core.py`, `_METHODS` registry at L1907). Three
interchangeable executors sit behind it: the PyO3 cdylib `dataworm._rust`
(built from `rust/` — `lib.rs`, `store.rs`, `refs.rs`, `pass.rs`, `query.rs`,
`semantic.rs`; `src/bin/main.rs` is the standalone `dataworm-core` binary
speaking the same contract on stdio), that binary itself for non-Python
callers, and the pure-Python fallback (`engine.py` / `graph.py` /
`persist.py`, forced by `--no-rust`). Backend parity is pinned by
`tests/test_rust_parity.py`, `tests/test_core_parity.py`,
`tests/test_traversal_parity.py`.

Filesystem events trigger incremental re-crawls: only affected fragments are
re-walked (shallow at the root fragment), unchanged files are reused by mtime,
and pass outputs come from the memo below (`dataworm/core.py` `_on_fs_events`
region; `dataworm/watcher.py`).

## Memoization design

Pass outputs are **content-addressed**: keyed by the sha256 `content_hash`
that the crawler recomputes whenever a file's mtime+size change, so the hash
itself is the dirty marker — a memo hit means "this exact content was already
extracted/embedded". The memo dict is `{"refs", "simhash", "embed"}`; it is
store-owned, threads across cycles and process restarts via the `memo` table
(`kind, key, value_json`, capped per kind) in `dataworm/persist.py`. Only
extraction and embedding are memoized; edge *resolution* always re-runs
against the current graph because resolution legitimately changes as nodes
appear/disappear (`dataworm/engine.py` module note, L49–64).

Known trade-off: `TfidfEmbedder` computes IDF over the batch it is given, so
a **partially** warm run embeds its misses against each other rather than the
full corpus — every reused vector stays byte-exact, but freshly embedded
files' IDF context differs. Fully-warm runs remain bit-identical to cold
output.

## Scale algorithms (exact at scale)

- **Simhash banding (near-duplicate candidates).** A 64-bit fingerprint splits
  into 4 disjoint 16-bit bands. Pigeonhole: any pair within hamming distance
  ≤ 3 differs in at most 3 bits, touching at most 3 bands — so at least one
  full band value is identical. Indexing fingerprints by band value therefore
  recalls *every* qualifying pair; the exact hamming check remains the
  verifier, applied once per deduped candidate pair
  (`dataworm/extractors/hashing.py` L13–18, `dataworm/engine.py` L231–247).
- **Inverted index (similarity candidates).** Terms index posting lists of
  nodes; any pair clearing a positive cosine threshold must share at least one
  term's posting list, so candidate generation has exact recall and only
  verification is scored over candidates (`dataworm/engine.py` L275, L373).
- Consequently `max_semantic_nodes` (50000) and `max_hashing_nodes` (100000)
  are memory safety valves, not correctness limits
  (`dataworm/config.py` L54–65). Exactness at scale is pinned by
  `tests/test_scale_exact.py` and `tests/test_scaling.py`.

## Federation model

Each crawled directory owns a **fragment**: one `GraphStore` persisted at
`<root>/.dataworm/graph.db`; the daemon aggregates counts across fragments
(`Core._aggregate_counts`). References that point into another fragment mint
**shadow nodes** — placeholder nodes id'd by absolute path — in both stores,
with a mirrored `references` edge on each side; reused fragments that were not
recrawled are left untouched unless cross-linking added shadow nodes to them
(`dataworm/core.py` L607–810). Impact BFS delegates to a single store when no
shadow dependents are involved and hops stores only through shadow nodes
(`Core._federated_impact_bfs`, L1255–1360). Federation behaviour is pinned by
`tests/test_federation.py` and `tests/test_multiroot.py`.

## Daemon topology

One detached daemon per project directory:

```
CLI / agent / browser ── ensure_daemon() ──► daemon (127.0.0.1:<port>)
   POST /rpc        JSON-RPC 2.0 → Core.call(method, params)
   GET  /api/<m>    REST wrapper → Core.call(method, query params)
   GET  /events     SSE stream (buffered replay + live EventBus events)
   GET  /           dashboard; GET /assets/<f> built-bundle statics
```

State lives next to the graph DB in `.dataworm/daemon.json`
(`{pid, port, token}`). Port selection tries 8765 then falls back to an
OS-assigned free port (`server.py` L108–121). Auth is a bearer token header
(`?token=` accepted for browser SSE/assets); loopback TCP only
(`dataworm/server.py` L279+, L293–303). `ensure_daemon` pings `/api/ping`,
respawns if dead, and reuses the warm graph otherwise. The dashboard bundle
lives at `dataworm/webapp/dist` (built from `web/`, SolidJS + Vite), with
`dataworm/live.py`'s inline page as automatic fallback and deny-by-default
path-checked asset serving.

## MCP era handling

The stdio server (`dataworm/mcp.py`) is dual-era per rev 2026-07-28:
modern stateless requests carry
`params._meta["io.modelcontextprotocol/protocolVersion"]` (unsupported →
`UnsupportedProtocolVersionError`, -32022); the mandatory `server/discover`
doubles as the era probe; list results carry `resultType` plus
`ttlMs`/`cacheScope`. Legacy handshake clients negotiate `initialize` against
2024-11-05…2025-11-25 and keep working unchanged (L66–105). Tools are the ten
`worm_*` entries of `build_tool_catalog()`; the Reflex Arc extension
`io.dataworm/changes` is advertised in discover capabilities and served by
`subscriptions/listen` → `notifications/dataworm/change` pushes.

## Journal schema

Append-only history per fragment DB (`dataworm/journal.py`):

```sql
CREATE TABLE IF NOT EXISTS journal(
    seq INTEGER PRIMARY KEY AUTOINCREMENT,
    ts REAL NOT NULL, kind TEXT NOT NULL, path TEXT NOT NULL, root TEXT NOT NULL,
    old_hash TEXT DEFAULT '', new_hash TEXT DEFAULT '',
    report_json TEXT NOT NULL, notified INTEGER DEFAULT 0);
CREATE INDEX IF NOT EXISTS idx_journal_ts ON journal(ts);
```

Reports are written before the `change` event fires; `notified` doubles as a
webhook delivery outbox (2xx marks delivered; failures retry next trigger —
best-effort v1). Saves rebuild the DB but carry journal rows forward verbatim
with original seq values, so `changes {since_seq}` paging survives re-crawls
and restarts (`journal.py` L107–142).

## Symlink/junction safety gates

Traversal is downward-only and never climbs above the crawl root.
`crawler._is_reparse_link()` treats a path as gated if it is a symlink **or**
an NTFS reparse point (`os.stat(follow_symlinks=False)`), because Windows
junctions are not `is_symlink()` yet still resolve to their target — gating on
symlinks alone once allowed junction cycles to recurse forever
(`dataworm/crawler.py` L26–37). Gated directories are recorded as nodes but
never descended into, which terminates cyclic junction pairs; the watcher's
snapshot builder applies the same gate (`dataworm/watcher.py` L168–196).
Federated fragment roots are canonicalised with realpath so short-name and
symlinked forms line up (`dataworm/core.py` L1715).

## Structured warnings contract

Traversal/stat/hash failures never silently produce empty hashes. The Rust
crawl snapshot returns structured warning dicts `{op, path, error}`;
`Core._rust_crawl` logs each and collects them, and the `crawl` result carries
the last 200 under `"warnings"` (Trust & Foresight). Consumers should treat
warnings as advisory detail about specific paths, not engine failure — a Rust
backend error still falls back to the Python path instead
(`dataworm/core.py` L900–909, L647–654).
