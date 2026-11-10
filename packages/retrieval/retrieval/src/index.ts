/**
 * Service Definition for workspace-scoped lexical, semantic, and hybrid retrieval.
 *
 * @module @deepseek-ai/dsh-retrieval
 */

import { Context, Service } from '@deepseek-ai/cordis'

declare module '@deepseek-ai/cordis' {
  interface Context {
    retrieval: RetrievalService
  }
}

/** Stable error codes for the retrieval contract. */
export type RetrievalErrorCode =
  | 'RETRIEVAL_ABORTED'
  | 'RETRIEVAL_INVALID_FILTER'
  | 'RETRIEVAL_INVALID_QUERY'
  | 'RETRIEVAL_INVALID_REQUEST'
  | 'RETRIEVAL_PROVIDER_FAILED'
  | 'RETRIEVAL_SEMANTIC_DISABLED'
  | 'RETRIEVAL_WORKSPACE_NOT_FOUND'

/** Typed retrieval failure. */
export class RetrievalError extends Error {
  constructor(
    message: string,
    readonly code: RetrievalErrorCode,
    options?: { cause?: unknown },
  ) {
    super(message, options)
    this.name = 'RetrievalError'
  }
}

/** Source scope of one indexed document. */
export type RetrievalDocumentScope = 'workspace' | 'session'

/** One searchable document owned by a workspace-scoped retrieval provider. */
export interface RetrievalDocument {
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

/** Request to add or replace documents in one workspace collection. */
export interface RetrievalIndexRequest {
  /** Absolute workspace root that owns the collection. */
  readonly workspaceRoot: string
  /** Documents to upsert. */
  readonly documents: readonly RetrievalDocument[]
}

/** Request to remove documents from one workspace collection. */
export interface RetrievalRemoveRequest {
  /** Absolute workspace root that owns the collection. */
  readonly workspaceRoot: string
  /** Document identities to remove; omit to apply the filter. */
  readonly ids?: readonly string[]
  /** Source scope filter. */
  readonly scope?: RetrievalDocumentScope
  /** Workspace-relative path filter. */
  readonly path?: string
  /** Session identity filter. */
  readonly sessionId?: string
}

/** Search mode. */
export type RetrievalSearchMode = 'lexical' | 'semantic' | 'hybrid'

/** Metadata filters applied before relevance ranking. */
export interface RetrievalSearchFilter {
  /** Source scopes to include. */
  readonly scope?: readonly RetrievalDocumentScope[]
  /** Workspace-relative paths to include. */
  readonly path?: readonly string[]
  /** Session identities to include. */
  readonly sessionId?: readonly string[]
  /** Event types to include. */
  readonly type?: readonly string[]
  /** Event surfaces to include. */
  readonly surface?: readonly string[]
  /** Inclusive minimum event time in Unix epoch milliseconds. */
  readonly timeFrom?: number
  /** Inclusive maximum event time in Unix epoch milliseconds. */
  readonly timeTo?: number
}

/** Request to search one workspace collection. */
export interface RetrievalSearchRequest {
  /** Absolute workspace root that owns the collection. */
  readonly workspaceRoot: string
  /** Query text interpreted as data, never as index syntax. */
  readonly query: string
  /** Search mode. */
  readonly mode: RetrievalSearchMode
  /** Maximum hits to return. */
  readonly limit: number
  /** Opaque continuation cursor from a previous page. */
  readonly cursor?: string
  /** Metadata filters applied before ranking. */
  readonly filter?: RetrievalSearchFilter
  /** Caller-supplied dense query vector, or omitted to use the configured embedder. */
  readonly vector?: readonly number[]
}

/** One retrieval hit. */
export interface RetrievalHit {
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

/** One cursor-paginated retrieval page. */
export interface RetrievalSearchPage {
  /** Hits for this page. */
  readonly items: readonly RetrievalHit[]
  /** Opaque continuation cursor, absent on the final page. */
  readonly nextCursor?: string
}

/** Aggregate retrieval statistics. */
export interface RetrievalStats {
  /** Number of open workspace collections. */
  readonly workspaces: number
  /** Total documents across open collections. */
  readonly documents: number
}

/** Workspace-scoped retrieval service. */
export abstract class RetrievalService extends Service {
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

export default RetrievalService
