---
description: "Mirrors live session events into the workspace retrieval collection so workspace search covers current history."
kind: "package-reference"
---

# @deepseek-ai/dsh-retrieval-session-index

English | [中文](README.zh.md)

## Summary

Use this package to keep a workspace retrieval collection current with live session history. Every committed session event with non-empty semantic text is upserted into the session workspace's collection by a stable `session:<id>:<seq>` document id. It complements the session-query service: ranked session search stays with the configured `ctx.sessionQuery` backend, while `workspace_search` sees the same session events through `ctx.retrieval`. Disposal drains per-session write chains before the plugin unloads.

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

Mount it after a retrieval provider, the session service, and the working-directory service. The shipped `dsh-base` bundle does this.

### Configuration

| Field | Default | Meaning |
|---|---|---|
| `maxTextBytes` | `8192` | Maximum semantic text bytes retained for one event |

### Behavior

- Only events whose header has a `cwd` are indexed; a session without an original project has no workspace collection.
- The workspace root is the session header's original `cwd`, not the effective working directory, so a session that changes directories keeps writing into its project.
- Documents carry `scope: 'session'`, `sessionId`, `seq`, `type`, `surface`, and `time`; the provider's metadata filters can narrow on any of them.
- Empty semantic text is skipped, including turn and step boundaries and unknown merge-extensible event types.
- Per-session writes are chained and a failure is logged without rejecting the committed session append.

### Failures

An indexing failure is logged with the session id and continues; the committed session append is never rolled back. Arguments are validated at load: `maxTextBytes` must be a positive safe integer.

-----

<a id="understand-the-implementation"></a>
## Understand the implementation

<details>
<summary>Implementation internals — click to expand</summary>

- **One event, one document.** The stable id makes repeated indexing idempotent and lets a later event replace an earlier one only when its seq matches.
- **Original project anchor.** The session header's `cwd` is the write root; the effective working directory is never used for storage.
- **Per-session write chains.** Each session owns one promise tail, so events append in log order without blocking the session append.
- **Disposal to quiescence.** The plugin's effect awaits every live session's tail before the provider closes.

### Source map

| File | Role |
|---|---|
| [`src/index.ts`](src/index.ts) | Event subscription, document projection, workspace anchoring, write chaining, disposal drain |

</details>

-----

<a id="further-exploration"></a>
## Further Exploration

- [dsh-retrieval](../retrieval/README.md) — the service this consumer writes to.
- [dsh-retrieval-zvec](../retrieval-zvec/README.md) — the shipped provider.
- [dsh-tool-retrieval](../tool-retrieval/README.md) — the model-facing search tool.

-----

<a id="model-experience"></a>
## Model Experience

None, as `dsh-retrieval-session-index` registers no tool, prompt section, or model-facing result; it only moves committed session text into a retrieval collection that `dsh-tool-retrieval` later renders.

#### KV Cache effect

No direct invalidation; the tool consumer owns any request-prefix changes.

## Known Limitations and Deferred Work

<a id="known-limitations-and-deferred-work"></a>

- **Live events only** — historical logs are not replayed at startup. A session opened after an empty collection misses its earlier events until another consumer indexes them.
- **No deduplication against persisted history** — the same event indexed by another consumer under a different id would appear twice.
- **Text is truncated** — an event above `maxTextBytes` is indexed as a prefix, so later words in that event are not searchable.

<a id="dev-note"></a>
### Dev Note

<details>
<summary>Working context for maintainers — click to expand</summary>

None.

</details>
