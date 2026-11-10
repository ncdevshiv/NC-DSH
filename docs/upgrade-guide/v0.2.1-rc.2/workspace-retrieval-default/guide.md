---
kind: upgrade-guide
description: "Shipped profiles now mount native workspace retrieval, and each workspace gains a repository-local .dsh/retrieval collection."
---

# Shipped profiles mount native workspace retrieval

English | [中文](guide.zh.md)

## Change

`dsh-base` now mounts `@deepseek-ai/dsh-retrieval-zvec`, `@deepseek-ai/dsh-tool-retrieval`, and `@deepseek-ai/dsh-retrieval-session-index` by default. The model receives two new tools: `workspace_search` searches project documents and prior session events, and `workspace_index` adds or replaces searchable documents by stable id. Live session events are mirrored into the same collection, so a search sees current history without a separate session-query backend.

Each workspace gains a repository-local `.dsh/retrieval` directory holding the zvec collection, its native logs, and an embedding cache if embeddings are enabled. The repository's `.gitignore` now ignores `/.dsh/`; other repositories and packaged deployments must ignore or exclude the same directory. The provider never writes outside the workspace root.

Embeddings are disabled in the shipped composition: lexical full-text search works immediately with no network access and no model download. A deployment that wants semantic or hybrid ranking sets `vectorDimension` and `embedding` on the `retrieval-zvec` row, which downloads a model into the same workspace directory on first use. Semantic and hybrid requests are rejected with `RETRIEVAL_SEMANTIC_DISABLED` until that is configured.

zvec holds an exclusive writable lock per collection. A second DSH process pointed at the same workspace fails to open the collection rather than sharing it.

## Migration

1. Update custom `cordis.yml` files that deliberately omit retrieval; nothing else needs to change. A composition that already set `session-query-sqlite` keeps its exact search behavior.

2. If the deployment must not create project-local data, disable the rows:

   ```yaml
   - id: retrieval-zvec
     disabled: true
   - id: tool-retrieval
     disabled: true
   - id: retrieval-session-index
     disabled: true
   ```

3. Confirm indexing and search from one conversation: call `workspace_index` with one document, then `workspace_search` with a word from it. The workspace's `.dsh/retrieval` directory appears, and the answer names the document path.
