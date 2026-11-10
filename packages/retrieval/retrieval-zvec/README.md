---
description: "The zvec-backed workspace retrieval provider: repository-local collections, native full-text ranking, and optional local embeddings."
kind: "package-reference"
---

# @deepseek-ai/dsh-retrieval-zvec

English | [中文](README.zh.md)

## Summary

Use this package to give a composition a real, native workspace retrieval engine. It registers `ctx.retrieval` with the official [`@zvec/zvec`](https://github.com/alibaba/zvec) Node binding: one collection per workspace under `<workspace>/.dsh/retrieval`, native full-text indexing, scalar filters, dense vectors, and reciprocal-rank-fusion hybrid search. Lexical search works with no embedding model; semantic and hybrid search use caller-supplied vectors or an explicitly configured local `fastembed` model whose weights stay in the same workspace directory. Shipped compositions enable the provider with embeddings off, so default retrieval never downloads a model.

## Table of Contents

- [Use this package](#use-this-package)
- [Understand the implementation](#understand-the-implementation)
- [Further Exploration](#further-exploration)
- [Model Experience](#model-experience)
- [Known Limitations and Deferred Work](#known-limitations-and-deferred-work)
- [Dev Note](#dev-note)

-----

<a id="use-this-package"></a>
## Use this package

Mount it after a composition has a workspace-oriented consumer. The shipped `dsh-base` bundle mounts it with `embedding: false` plus `dsh-tool-retrieval` and `dsh-retrieval-session-index`.

### Minimal configuration

```yaml
- name: '@deepseek-ai/dsh-retrieval-zvec'
  config:
    # Defaults: dataDir `.dsh`, collection `retrieval`, embedding false.
    vectorDimension: 384
```

| Field | Default | Meaning |
|---|---|---|
| `dataDir` | `.dsh` | Repository-local directory holding the collection, logs, and model cache |
| `collectionName` | `retrieval` | zvec collection directory name inside `dataDir` |
| `vectorDimension` | omitted | Enables dense vectors at the given dimension; omit to run lexical-only |
| `embedding` | omitted | `false` disables local embedding; an object selects a `fastembed` model, `maxLength`, and `batchSize` |
| `maxLimit` | `100` | Largest accepted search page |
| `queryThreads` / `optimizeThreads` | omitted | zvec native thread counts; omitted lets the engine decide |
| `logLevel` | `ERROR` | zvec native log level written under `<dataDir>/<collection>-logs` |

When `embedding` is set and `vectorDimension` is omitted, the provider uses the model's dimension (384 for the default English BGE small v1.5). An explicit dimension must match the selected model.

### Repository-local data

The provider never writes outside the workspace root it is given. A collection path, a native log directory, and an embedding cache all share `<workspace>/<dataDir>`. The `.dsh/` directory is ignored by this repository's `.gitignore`; deployments should ignore or exclude it the same way. The first open creates the directory tree; the provider never falls back to the process working directory.

### Semantics and lifecycle

Lexical search quotes the query as one literal phrase, so FTS syntax cannot escape the text. Filters become zvec boolean expressions over `scope`, `path`, `session_id`, `seq`, `type`, `surface`, and `time`. Semantic search requires `vectorDimension`; hybrid search runs one FTS subquery and one vector subquery and fuses them with RRF rank constant 60. Writes are serialized per provider and synchronous on the zvec side; `flush()` awaits the accepted write tail, and `close()` drains it before closing every collection.

zvec holds an exclusive writable lock per collection path. A second writable open of the same collection — in this process or another — fails loudly, so one DSH process owns a workspace collection at a time.

### Failures

Provider failures are `RetrievalError`s with the service's stable codes. A missing or locked collection fails with `RETRIEVAL_PROVIDER_FAILED`; a semantic request without a dimension or embedder fails with `RETRIEVAL_SEMANTIC_DISABLED`; invalid vectors, cursors, or limits fail with `RETRIEVAL_INVALID_REQUEST`.

-----

<a id="understand-the-implementation"></a>
## Understand the implementation

<details>
<summary>Implementation internals — click to expand</summary>

- **One medium, one writer.** zvec's native exclusive lock is the ownership primitive; the provider adds per-workspace collection handles on top.
- **Repository identity.** The caller supplies the absolute workspace root; the provider resolves it, verifies it is a directory, and builds all paths beneath it.
- **Optional semantics.** The collection schema includes a vector field only when a dimension is configured, so a lexical-only deployment pays no vector memory and cannot accidentally accept vectors.
- **Serialized accepted writes.** Every mutation joins one promise chain; a rejected operation does not poison later accepted operations.

### Source map

| File | Role |
|---|---|
| [`src/index.ts`](src/index.ts) | Provider configuration, collection lifecycle, filtering, search modes, optional embedding runtime |

</details>

-----

<a id="further-exploration"></a>
## Further Exploration

- [dsh-retrieval](../retrieval/README.md) — the service definition this provider implements.
- [dsh-retrieval-session-index](../retrieval-session-index/README.md) — the consumer that mirrors live session events into this provider.
- [dsh-tool-retrieval](../tool-retrieval/README.md) — the model-facing search and index tools.

-----

<a id="model-experience"></a>
## Model Experience

Indirectly, through `dsh-tool-retrieval`, which renders workspace hits and failures as bounded tool results.

#### KV Cache effect

No direct invalidation; the named consumer owns any request-prefix changes.

## Known Limitations and Deferred Work

<a id="known-limitations-and-deferred-work"></a>

- **One writable process per workspace** — the native exclusive lock means a second DSH process cannot write the same workspace collection; it fails rather than sharing.
- **No automatic reconciliation** — the provider indexes what consumers send; it does not scan the workspace or replay historical session logs by itself. `dsh-retrieval-session-index` covers live events only.
- **Local embeddings are optional and network-backed on first use** — configuring `embedding` downloads a model from Hugging Face into the workspace cache; deployments without network access must pre-seed the cache or use caller-supplied vectors.
- **Model cache is per workspace** — two projects embedding with the same model keep separate copies. This is deliberate so every model file stays inside the project that used it.

<a id="dev-note"></a>
### Dev Note

<details>
<summary>Working context for maintainers — click to expand</summary>

None.

</details>
