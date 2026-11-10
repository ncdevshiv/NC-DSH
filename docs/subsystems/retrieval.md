# Retrieval

English | [中文](retrieval.zh.md)

Workspace-scoped retrieval over files and session events. The [Service Definition package](../../packages/retrieval/retrieval) owns the request and result vocabulary, ranking modes, filters, and error codes, while the [zvec provider](../../packages/retrieval/retrieval-zvec) owns repository-local collections, native full-text and vector search, and optional local embeddings.

Source: [`packages/retrieval/retrieval/src/index.ts`](../../packages/retrieval/retrieval/src/index.ts) · [`packages/retrieval/retrieval-zvec/src/index.ts`](../../packages/retrieval/retrieval-zvec/src/index.ts)

## The service: `ctx.retrieval`

`RetrievalService` is one workspace-scoped seam. Every request names an absolute workspace root, so one provider instance can serve several projects without process-global current-workspace state. Providers own the medium; consumers never reach a specific engine.

```ts type-equiv
/** Workspace-scoped retrieval service. */
abstract class RetrievalService extends Service {
  constructor(ctx: Context) {
    super(ctx, 'retrieval')
  }

  /**
   * Add or replace documents in one workspace collection.
   * @param request - Workspace root and documents to upsert.
   * @param signal - cancellation for embedding and provider work.
   * @returns resolution after the provider acknowledged durability.
   */
  abstract indexDocuments(request: RetrievalIndexRequest, signal?: AbortSignal): Promise<void>

  /**
   * Remove documents from one workspace collection.
   * @param request - Workspace root and either document ids or a metadata filter.
   * @param signal - cancellation for provider work.
   * @returns resolution after the provider acknowledged durability.
   */
  abstract removeDocuments(request: RetrievalRemoveRequest, signal?: AbortSignal): Promise<void>

  /**
   * Search one workspace collection.
   * @param request - Query, mode, filters, and page controls.
   * @param signal - cancellation for embedding and provider work.
   * @returns one ranked page of hits.
   */
  abstract search(request: RetrievalSearchRequest, signal?: AbortSignal): Promise<RetrievalSearchPage>

  /**
   * Read aggregate statistics for open workspace collections.
   * @returns workspace and document counts.
   */
  abstract stats(): Promise<RetrievalStats>

  /**
   * Wait for every accepted write to reach provider durability.
   * @returns resolution after the current write queue drains.
   */
  abstract flush(): Promise<void>

  /**
   * Close every open collection and release provider resources.
   * @returns resolution after disposal reaches quiescence.
   */
  close(): Promise<void> {
    return Promise.resolve()
  }
}
```

## Records

`RetrievalDocument` is the input record. `RetrievalHit` is the output record; it preserves the source scope and optional path or Session metadata. A dense vector rides either record only when a deployment configures a vector dimension.

```ts type-equiv
/** One searchable document owned by a workspace-scoped retrieval provider. */
interface RetrievalDocument {
  /** Stable document identity within the workspace collection. */
  readonly id: string
  /** Source scope that owns the document. */
  readonly scope: RetrievalDocumentScope
  /** Searchable text. */
  readonly text: string
  /** Workspace-relative file path for workspace documents. */
  readonly path?: string
  /** Session identity for session documents. */
  readonly sessionId?: string
  /** Event sequence for session documents. */
  readonly seq?: number
  /** Session event type for session documents. */
  readonly type?: string
  /** Session event surface for session documents. */
  readonly surface?: string
  /** Event time in Unix epoch milliseconds. */
  readonly time?: number
  /** Dense vector for semantic and hybrid search. */
  readonly vector?: readonly number[]
}
```

```ts type-equiv
/** One retrieval hit. */
interface RetrievalHit {
  /** Stable document identity. */
  readonly id: string
  /** Source scope. */
  readonly scope: RetrievalDocumentScope
  /** Searchable text. */
  readonly text: string
  /** Provider relevance score. */
  readonly score: number
  /** Workspace-relative file path for workspace documents. */
  readonly path?: string
  /** Session identity for session documents. */
  readonly sessionId?: string
  /** Event sequence for session documents. */
  readonly seq?: number
  /** Session event type for session documents. */
  readonly type?: string
  /** Session event surface for session documents. */
  readonly surface?: string
  /** Event time in Unix epoch milliseconds. */
  readonly time?: number
}
```

`RetrievalSearchFilter` ANDs supplied fields: scopes, paths, session ids, event types, event surfaces, and an inclusive time range. Arrays within one field are ORed.

## Modes

`lexical` quotes the query as one literal phrase and ranks with the native full-text index. `semantic` requires a configured vector dimension and either a caller-supplied query vector or a configured local embedder. `hybrid` runs one full-text subquery and one vector subquery and fuses them with reciprocal rank fusion (rank constant 60). A deployment without a vector dimension rejects semantic and hybrid calls with `RETRIEVAL_SEMANTIC_DISABLED` rather than silently falling back.

## Repository-local storage

The shipped provider resolves `<workspace>/<dataDir>` for every collection, native log directory, and embedding model cache; `dataDir` defaults to `.dsh`. A second writable open of the same collection fails because zvec holds a native exclusive lock, so one DSH process owns a workspace collection at a time. The provider never falls back to the process working directory.

<!-- BEGIN GENERATED cordis-surface (gen-cordis-catalog.ts) — do not edit between markers -->

<a id="cordis-surface"></a>

## Cordis API

Generated from source by `scripts/gen-cordis-catalog.ts` (verified fresh by `bun run verify-cordis-catalog` in doc-sync; regenerate with `bun run gen-cordis-catalog`) — the language sides differ only in locale-specific paired document paths. Signature blocks use a `ts cordis-catalog` fence and keep the original source JSDoc; dispatch modes are defined in the [primer](../cordis-primer.md#dispatch-modes), and the framework-inherited `ctx` API lives in [cordis-api/inherited.md](../cordis-api/inherited.md).

<a id="ctxretrieval--retrievalservice-abstract-seam"></a>

### `ctx.retrieval` — `RetrievalService` (abstract seam)

Workspace-scoped retrieval service.

```ts cordis-catalog
/**
 * Add or replace documents in one workspace collection.
 * @param request - Workspace root and documents to upsert.
 * @param signal - cancellation for embedding and provider work.
 * @returns resolution after the provider acknowledged durability.
 */
abstract indexDocuments(request: RetrievalIndexRequest, signal?: AbortSignal): Promise<void>

/**
 * Remove documents from one workspace collection.
 * @param request - Workspace root and either document ids or a metadata filter.
 * @param signal - cancellation for provider work.
 * @returns resolution after the provider acknowledged durability.
 */
abstract removeDocuments(request: RetrievalRemoveRequest, signal?: AbortSignal): Promise<void>

/**
 * Search one workspace collection.
 * @param request - Query, mode, filters, and page controls.
 * @param signal - cancellation for embedding and provider work.
 * @returns one ranked page of hits.
 */
abstract search(request: RetrievalSearchRequest, signal?: AbortSignal): Promise<RetrievalSearchPage>

/**
 * Read aggregate statistics for open workspace collections.
 * @returns workspace and document counts.
 */
abstract stats(): Promise<RetrievalStats>

/**
 * Wait for every accepted write to reach provider durability.
 * @returns resolution after the current write queue drains.
 */
abstract flush(): Promise<void>

/**
 * Close every open collection and release provider resources.
 * @returns resolution after disposal reaches quiescence.
 */
close(): Promise<void>
```

Source: [`packages/retrieval/retrieval/src/index.ts`](../../packages/retrieval/retrieval/src/index.ts)
<!-- END GENERATED cordis-surface -->
