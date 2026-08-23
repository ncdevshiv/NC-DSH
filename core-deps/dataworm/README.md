# DataWorm

DataWorm is a **self-aligning directory graph**. Point it at a directory and it
crawls *downward* through every folder and file, turning each into a node in a
typed link graph, then realigns pass after pass until the graph stops changing
(a fixed point). Four link dimensions connect those nodes:

| Dimension      | Meaning                                                        |
|----------------|----------------------------------------------------------------|
| `contains`     | Structural hierarchy: a directory contains a child dir/file.    |
| `references`   | Content links: imports, requires, markdown/relative paths.      |
| `duplicate_of` | Identical (sha256) or near-identical (simhash) content.         |
| `similar_to`   | Semantically related (embedding cosine above a threshold).      |

Query the settled graph for **linkage**: before an edit, ask *"what does this
touch?"* and get the blast radius — so an agent (or you) can confirm the edit
is safe instead of guessing.

## ✨ Feature highlights

| Feature | What it means |
|---------|---------------|
| Four-dimensional graph | `contains`, `references`, `duplicate_of`, `similar_to` edges over one node set. |
| Blast radius | `impact` returns direct + transitive dependents of any file before you edit it. |
| plan_edit what-if | Simulate proposed content: links gained/lost, new dangling references, dependents, duplication radar — dry-run, never writes disk. |
| Reflex Arc live deltas | Append-only change journal plus push: MCP `subscriptions/listen` or HTTP webhook fire as files change. |
| Federated stores | One graph DB per crawled directory ("fragment"); cross-directory references become shadow-node links; impact hops fragments automatically. |
| Rust + Python dual engine | PyO3 cdylib fast path, pure-Python fallback; parity tests pin identical results between them. |
| Exact-at-scale algorithms | Simhash banding (pigeonhole over 4×16-bit bands) and inverted-index candidates recall every qualifying pair; memory caps are valves, not correctness limits. |
| MCP dual-era server | Serves rev 2026-07-28 stateless clients *and* legacy handshake clients (≤ 2025-11-25) from the same stdio process. |
| SolidJS live dashboard | Served by the daemon itself; SSE-driven graph view, change feed, node detail drawer. |

## 🚀 Install (Windows today)

```powershell
powershell -ExecutionPolicy Bypass -File scripts\install-dw.ps1
```

- Installs two identical global commands, **`dataworm`** and **`dw`**, into
  `%USERPROFILE%\.local\bin` (the script warns if that dir is not on PATH).
- Requires **uv** (<https://docs.astral.sh/uv/>). **git** is needed later for
  branch updates (`dw up <branch>`). A **Rust toolchain (cargo)** builds the
  bundled extension at install time and is picked up automatically from PATH
  or `~\.cargo\bin`.
- The script records the checkout into `%USERPROFILE%\.dataworm\source.txt`.

Platform note: the code is cross-platform — CI runs Ubuntu, Windows and macOS
across Python 3.10 / 3.12 / 3.14 — but the global installer is PowerShell-only
today. Elsewhere, install from a checkout: `pip install -e .` or
`uv tool install --force --editable .`.

## ⚡ Quickstart

From any directory:

```
dw
```

Bare summon = full launch of *this* directory: crawl once, start watching,
ensure the background daemon, open the dashboard at
`http://127.0.0.1:8765/` (or the next free port). The port and bearer token
are recorded in `<dir>\.dataworm\daemon.json`. **Ctrl+C soft-closes**: it stops
the daemon this directory started, leaves all graph data intact, exits 130.
(A zero-args call with piped stdin shows help instead of launching.)

| Command | Description |
|---------|-------------|
| `dw [dir]` / `dw init [dir]` | Summon: crawl once, watch, ensure daemon, open dashboard. |
| `dw crawl <dir>` | Build/refresh the graph (`--watch`, `--live`, `--web`). |
| `dw watch [dir]` | Keep watching; `--webhook URL` pushes change reports live. |
| `dw impact <path>` | Blast radius: direct + transitive dependents of a file. |
| `dw context <path>` | Metadata + links across all dimensions + impact for a file. |
| `dw neighbors <path>` | Nodes within N hops (`--type <dim>`, `--depth N`). |
| `dw search <text>` | Substring search over indexed node paths (`--limit`). |
| `dw summary` | Graph stats + convergence info. |
| `dw status` | Daemon liveness, engine backend, watched roots. |
| `dw stop` | Shut the daemon down. |
| `dw up [branch]` | Self-update + reinstall (see below). |
| `dw update` | Legacy pip-based upgrade of the installed distribution. |
| `dw mcp` | Run the MCP stdio server (for Claude Desktop / Cursor / …). |

Shared flags: `--json` machine-readable output; `--db` / `--out` graph DB path
(default `<dir>/.dataworm/graph.db`); `--no-daemon` run in-process;
`--no-rust` force the Python backend.

The daemon also speaks plain JSON methods beyond the CLI — `ping`, `changes`,
`plan_edit`, `roots`, `signature`, `watched`, `configure_webhook`, `unwatch`,
`hash_pass`, `extract_refs`, `shutdown` — via `POST /rpc` or
`GET /api/<method>`; the agent-facing ones are exposed over MCP as
`worm_changes` / `worm_plan_edit`.

## 🔄 Updating

- **`dw up`** (= `dw up main`), **`dw up dev`**, **`dw up <any-branch>`**:
  shallow-syncs `github.com/ncdevshiv/dataworm@<branch>` into
  `~\.dataworm\src\<branch>` (the build cache there survives, so recompiles
  stay fast) and reinstalls via `uv tool install --force`. Repo override:
  `DATAWORM_REPO` env var or `~\.dataworm\repo.txt`.
- **Detached handoff:** when `dw up` runs from the very installation being
  replaced, it exits immediately and a detached helper performs the swap.
  Follow `%TEMP%\dw-up.log`, then check `dw status`. Otherwise the install
  streams live.
- **`dw up --from <dir>`** installs a local checkout instead; git is skipped.
- Updates never touch project data: `<dir>\.dataworm\` graphs belong to your
  projects, not the install. Any running daemon is stopped first; run `dw`
  again in your project afterwards to relaunch on the new build.
- `update` is the older pip-based path: `dataworm update [--from <spec>]
  [--no-restart]`, logging to `%TEMP%\dataworm-update.log`.

## 🤖 MCP integration

`dataworm mcp` runs the Model Context Protocol stdio server (newline-delimited
JSON-RPC 2.0; stdlib only). Register it in `claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "dataworm": {
      "command": "dataworm",
      "args": ["mcp", "--db", "C:/path/to/project/.dataworm/graph.db"]
    }
  }
}
```

Ten tools: `worm_crawl`, `worm_impact`, `worm_context`, `worm_neighbors`,
`worm_search`, `worm_summary`, `worm_watch`, `worm_unwatch`, `worm_changes`,
and `worm_plan_edit` (the what-if simulator). The agent loop: call
`worm_impact` **before every edit** — an empty direct/transitive blast radius
means the change is safe.

For live updates without polling, send a `subscriptions/listen` request with
`{"types": ["io.dataworm/change"]}` — the server acks once with
`{"resultType": "complete", "subscriptionId": ..., "acceptedTypes": [...]}`,
then pushes one `notifications/dataworm/change` message per detected file
change until you re-call with `{"unsubscribe": "<id>"}`. Polling works too:
`worm_changes {since_seq?, limit?}` pages the journal (`last_seq` back in as
`since_seq`).

Protocol era statement — dual-era per revision
[2026-07-28](https://modelcontextprotocol.io/specification/2026-07-28):
modern stateless requests declare their version via
`params._meta["io.modelcontextprotocol/protocolVersion"]`; unsupported
versions get `UnsupportedProtocolVersionError` (-32022); the mandatory
`server/discover` RPC is implemented and doubles as the stdio era probe; list
results carry `resultType` / `ttlMs` / `cacheScope`. Legacy handshake clients
(`initialize`, ≤ 2025-11-25 — what current client releases ship) keep working
unchanged.

## 🖥️ Dashboard

Served by the daemon itself at `/` — no separate server. The SolidJS bundle
(source in `web/`, built into `dataworm/webapp/dist`) renders: connection
header, graph summary panel, a live zoomable canvas graph, the Reflex Arc
change feed, a per-node detail drawer, and an activity console. Everything
streams over SSE (`GET /events`, browser auth via `?token=`). If the built
bundle is absent, the daemon automatically falls back to the legacy inline
page (`dataworm/live.py`) — same data, plainer UI.

## 🏗️ Architecture

One JSON contract — `{method, params} → {result | error}` — executed by a
single dispatcher (`Core.call()`), backed by three interchangeable engines:
the PyO3 cdylib `dataworm._rust`, the standalone `dataworm-core` binary, and
the pure-Python fallback. The engine loops four passes (crawl, references,
hashing, semantic) until the graph signature stabilizes. Each crawled
directory is its own federated fragment store; one detached daemon per project
serves JSON-RPC, REST, SSE, and the dashboard over loopback TCP with bearer
auth. Deep dive: [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## ⚠️ Limits & platform notes

- Files over **2 MiB** (`max_content_bytes`) are catalogued but their content
  is not parsed for references/embeddings/hashes.
- Default ignore rules skip noise dirs (`.git`, `node_modules`, `__pycache__`,
  `.dataworm`, …) and binary/media globs; tune via `Config`.
- Reference extraction is **regex-per-language** (Python/JS imports,
  `require()`, markdown links, relative paths), not AST parsing — resilient
  and dependency-free, occasionally wrong inside comments or strings.
- **Junctions/symlinks (Windows):** reparse-point directories are recorded as
  nodes but never descended into; cyclic junction pairs terminate safely.
  This gate matters because NTFS junctions are not `is_symlink()`.
- Webhook delivery is **best-effort v1**: reports queue in the journal's
  delivery outbox and retry on the next trigger; there is no backoff or
  dead-letter handling yet.
- Semantic/hashing caps (50k vectors, 100k fingerprints) bound memory only;
  candidate generation stays exact at any scale.
