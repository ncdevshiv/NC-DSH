# Agent Note: Repository-local workspace retrieval through zvec

Status: implemented

English | [中文](2026-11-08-repository-local-workspace-retrieval.zh.md)

## Problem

The harness had two retrieval paths and no shared one. Session history search existed only as a SQLite FTS backend that shipped disabled (`openAt: never`), and model file discovery was lexical ripgrep with no way to search project content by meaning. A user asking the agent to find relevant prior work or project text had no default capability: content search was off, `glob`/`grep` required an exact pattern, and nothing indexed session events for later search.

The request was to integrate the native [zvec](https://github.com/alibaba/zvec) engine, keep it on by default, and confine every zvec-owned file — collections, indexes, native logs, and embedding caches — to each project's repository-local data directory. It also asked for honest with/without measurements covering quality, token cost, latency, and trade-offs.

## Decision

### A workspace retrieval seam, not a second session backend

`@deepseek-ai/dsh-retrieval` declares `ctx.retrieval`: index, remove, search, stats, flush, close. Every request names the absolute workspace root, so one provider instance serves several projects without process-global current-workspace state. The seam owns records, modes (`lexical`, `semantic`, `hybrid`), metadata filters, cursors, and stable error codes; providers own the medium.

This is separate from `ctx.sessionQuery`, which keeps exact history reads, filters, traces, and its SQLite FTS ranking. Replacing that backend would have forced exact-search call sites through vector ranking and changed cursor semantics; the new seam lets retrieval carry file and session text without touching session-history authority.

### zvec as the shipped provider

`@deepseek-ai/dsh-retrieval-zvec` registers `ctx.retrieval` with the official Node binding. Each workspace owns one collection at `<root>/<dataDir>/<collectionName>`, with `dataDir` defaulting to `.dsh`; native log files and the fastembed model cache share that directory. The provider validates the root, chunks writes to zvec's 1024-document native limit, serializes accepted writes on one promise chain, exposes `flush()`, and closes every collection on disposal.

The collection always has a full-text index over `text` plus invert indexes over `scope`, `path`, `session_id`, `seq`, `type`, `surface`, and `time`. A vector field exists only when `vectorDimension` is configured, so a lexical-only deployment pays no vector memory and cannot silently accept vectors. `hybrid` runs one FTS subquery and one vector subquery and fuses them with RRF at rank constant 60. Query text is quoted as one literal FTS phrase so index syntax stays inert.

zvec holds an exclusive writable lock per collection path. A second writable open fails instead of sharing, so one DSH process owns a workspace collection at a time; the provider documents that failure rather than adding a silent read-only fallback.

### Embeds are optional and never download at startup

zvec does not embed. The shipped `dsh-base` composition mounts the provider with `embedding: false`, so lexical workspace search is available immediately with no network access and no model download. Semantic and hybrid requests fail with `RETRIEVAL_SEMANTIC_DISABLED` until a deployment configures `vectorDimension` and either supplies caller vectors or enables the `fastembed` model, whose weights stay in `<workspace>/.dsh/retrieval-models`. A configured model and an explicit dimension must agree, which is checked at load.

### One model tool pair and one live indexer

`@deepseek-ai/dsh-tool-retrieval` registers `workspace_search` and `workspace_index`. Both anchor on `agent.session.header.cwd`, not the effective working directory, so a Session that changes directories still reads and writes its repository's collection. The search tool hides cursors, bounds hits by `maxResults`, renders short snippets, and translates provider failures into `RETRIEVAL_TOOL_FAILED`.

`@deepseek-ai/dsh-retrieval-session-index` subscribes to `session/event`, projects each committed event with semantic text into a `session:<id>:<seq>` document, and writes per-session promise chains that disposal drains. It stores under the session header's original `cwd`, skips events without text, and truncates text to `maxTextBytes`.

All three rows and the provider mount unconditionally in `dsh-base`; the Web bundle inherits them. The repository's `.gitignore` ignores `/.dsh/`, and the upgrade guide names the directory and the opt-out rows.

## Alternatives considered

- **Replace `session-query-sqlite` with a zvec `SessionQueryEngine`.** Rejected: exact history cursor generations, reconciliation, and snippet semantics are load-bearing for the Web sidebar and `tool-session-query`; a vector engine would change rank order and cursor meaning. The retrieval seam adds a capability instead of swapping an authority.
- **Enable local embeddings by default.** Rejected: a first-run Hugging Face download would make a shipped default depend on network access and disk, and would hide model-download latency inside the first search. Lexical-only default keeps the activation side-effect free and lets semantic deployments opt in explicitly.
- **Load a model with `fastembed` at startup even when unused.** Rejected: the optional dependency and model load cost are paid only when a semantic request actually needs them; the shipped composition never pays either.
- **Store the collection under the Harness home.** Rejected by the request: project data must live in the project. The trade-off is duplicate model caches across projects, recorded as a limitation.
- **Echo exact-search semantics into `workspace_search`.** Rejected: `glob`/`grep` remain exact lexical tools with their own contract; `workspace_search` is ranked retrieval and says so in its guidance. Callers that need a literal scan keep using grep or `filterEvents()`.
- **Fall back to the effective working directory for storage.** Rejected: after a `cd`, writes would land outside the repository that owns the Session; anchoring on the header keeps repository identity stable.

## Consequences

Workspace retrieval is a real default capability: the model can search current session text and indexed project documents without configuration, and every byte stays under the workspace's `.dsh/` tree. Exact history search keeps its SQLite contract; the two search surfaces answer different questions.

On the matched synthetic corpus, the zvec provider matches the in-process lexical and brute-force vector references exactly: recall 0.40 (25 relevant documents capped at top 10), precision 1.0, reciprocal rank 1.0, and hit rate 1.0 for lexical, semantic, and hybrid modes on both sides, with 640 estimated tokens per query from returned snippets on both sides. Indexing 2,000 documents of 80 words takes about 0.4 s; per-query medians are about 0.36 ms lexical, 0.27 ms semantic, and 0.19 ms hybrid, and close takes about 0.41 s. The reference rows are deliberately in-process and unindexed, so those timing ratios describe this fixture, not a product-level speed claim; the cost the benchmark proves is token parity and quality parity, and the latency numbers are recorded as budgets rather than a comparison to SQLite.

The write path costs one native synchronous batch per document slice and a full close pass; both are measured by the gate. Two limitations are explicit: one writable process per workspace, because of the native lock, and no automatic reconciliation — the provider indexes what consumers send and the consumer currently mirrors live events only, not historical logs.

Verification lives in the retrieval packages' suites (provider lifecycle, persistence and reopen, filters, cursor paging, semantic and hybrid with caller vectors, embedding-model wiring through a mocked fastembed boundary, negative paths, and a real Loader composition with a `cordis.yml` tree), the base bundle's composition assertions, the headless profile's keyless smoke asserting `workspace_search` and `workspace_index` in the persisted request header, and the required `workspace-retrieval` benchmark gate.

## Related

- [Retrieval subsystem](../../../../docs/subsystems/retrieval.md)
- [Session Query subsystem](../../../../docs/subsystems/session-query.md)
- [Storage subsystem](../../../../docs/subsystems/storage.md)
