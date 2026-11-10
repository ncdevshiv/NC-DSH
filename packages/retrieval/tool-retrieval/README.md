---
description: "Model-facing workspace search and indexing tools over the workspace retrieval service."
kind: "package-reference"
---

# @deepseek-ai/dsh-tool-retrieval

English | [中文](README.zh.md)

## Summary

`dsh-tool-retrieval` registers two model-facing tools over `ctx.retrieval`: `workspace_search` finds project files and prior session events by text or meaning, and `workspace_index` adds or replaces searchable documents by stable id. Both are workspace-scoped through the caller's original project directory, so a session that changes its working directory still reads and writes the collection owned by its repository. The tools return bounded plain text and use the provider's own lexical, semantic, or hybrid ranking.

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

Mount it after a retrieval provider and the working-directory service. The shipped `dsh-base` bundle mounts it alongside `dsh-retrieval-zvec` and `dsh-retrieval-session-index`.

### The tools

| Tool | Arguments | Behavior |
|---|---|---|
| `workspace_search` | `query`, `mode?`, `path?`, `session_id?`, `event_types?`, `event_surfaces?`, `time_from?`, `time_to?` | Searches the caller workspace and returns ranked hits with location, scope, score, and a bounded snippet. `mode` defaults to `lexical`; `semantic` and `hybrid` require an embedding-capable deployment |
| `workspace_index` | `documents[]` | Upserts documents by stable id; each document carries text plus optional path, session metadata, time, and a caller-supplied vector |

`workspace_search` uses the maximum result count configured on this package and never returns more than that page. A final page has no continuation notice; an earlier page tells the model more matches exist and to narrow the query.

### Configuration

| Field | Default | Meaning |
|---|---|---|
| `maxResults` | `20` | Maximum hits returned by one search |
| `timeoutMs` | `30000` | Cooperative deadline for either tool |

### Failures

Invalid arguments fail with `RETRIEVAL_TOOL_INVALID_ARGUMENTS` before the provider is called. A missing agent-bound caller fails with `RETRIEVAL_TOOL_MISSING_AGENT`. Provider failures are logged and translated to `RETRIEVAL_TOOL_FAILED` so internal paths and native error text do not reach the model.

-----

<a id="understand-the-implementation"></a>
## Understand the implementation

<details>
<summary>Implementation internals — click to expand</summary>

- **Original project anchoring.** The tools read `agent.session.header.cwd`, not the effective working directory, so index and search data stays inside the repository even after `cd`.
- **Provider-agnostic.** The tools call only `ctx.retrieval`; they do not know zvec, SQLite, or any other medium.
- **Bounded output.** Each hit contributes a short location, score, and snippet; the complete result never exceeds `maxResults`.

### Source map

| File | Role |
|---|---|
| [`src/index.ts`](src/index.ts) | Tool schemas, argument normalization, workspace anchoring, retrieval calls, text rendering |

</details>

-----

<a id="further-exploration"></a>
## Further Exploration

- [dsh-retrieval](../retrieval/README.md) — the service these tools consume.
- [dsh-retrieval-zvec](../retrieval-zvec/README.md) — the shipped provider.
- [dsh-retrieval-session-index](../retrieval-session-index/README.md) — the live session-event indexer.

-----

<a id="model-experience"></a>
## Model Experience

### System prompt

#### What the model sees

A `tool:retrieval` section states when to use each tool, that lexical mode matches text exactly, and that semantic and hybrid modes require an embedding-capable deployment.

##### Retrieval guidance

```markdown
Use workspace_search to find relevant project files and prior session events. Lexical mode matches text exactly; semantic and hybrid modes rank by meaning and require an embedding-capable deployment. Results are workspace-scoped and each hit names its source.
Use workspace_index to add or refresh searchable workspace documents. Pass stable ids; re-indexing the same id replaces it.
```

#### Token effect

Fixed guidance cost on every request where the section is visible.

#### KV Cache effect

Prefix-stable while the section and tool visibility are unchanged. Tool restrictions, activation, or disposal may invalidate reuse from the first changed section.

### Tool schemas

#### What the model sees

The generated [`workspace_search` and `workspace_index` schemas](../../../docs/tool-catalog.md#deepseek-aidsh-tool-retrieval) describe their arguments and default mode. Results are plain text, not structured JSON.

#### Token effect

Fixed schema cost while the tools are visible; each result adds bounded retained tokens.

#### KV Cache effect

Prefix-stable while tool visibility and definitions are unchanged.

### Results

#### What the model sees

`Workspace matches (N):` followed by numbered hits with location, scope, score, and snippet; `No workspace matches found.` for an empty page; `Indexed N workspace documents.` for a successful index.

#### Token effect

Results are bounded by `maxResults`, each snippet is bounded by 240 Unicode code points, and the retained call plus result stay in history until compaction.

#### KV Cache effect

Append-only; newly visible content follows the reusable request prefix and does not invalidate existing KV-cache entries.

### Tool errors

#### What the model sees

Argument failures are `Error: <message>` with structured `RETRIEVAL_TOOL_INVALID_ARGUMENTS`, `RETRIEVAL_TOOL_MISSING_AGENT`, or `RETRIEVAL_TOOL_FAILED` metadata.

#### Token effect

Only a failing call adds these retained tokens.

#### KV Cache effect

Append-only.

## Known Limitations and Deferred Work

<a id="known-limitations-and-deferred-work"></a>

- **No automatic indexing of workspace files** — `workspace_index` is explicit; the tools do not scan the repository or watch for changes by themselves.
- **No continuation follow-up** — a page that is not final only tells the model to narrow; it does not expose the cursor, so one tool call returns at most `maxResults` hits.
- **Semantic modes are deployment-dependent** — a lexical-only deployment rejects semantic and hybrid requests rather than silently falling back to text.

<a id="dev-note"></a>
### Dev Note

<details>
<summary>Working context for maintainers — click to expand</summary>

None.

</details>
