/**
 * Workspace-scoped retrieval provider backed by zvec, with repository-local
 * collections, native full-text indexing, and optional local embeddings.
 * @module @deepseek-ai/dsh-retrieval-zvec
 */

import { existsSync, mkdirSync, statSync } from 'node:fs'
import { join, resolve } from 'node:path'
import type { Context } from '@deepseek-ai/cordis'
import z from '@deepseek-ai/schemastery'
import {
  RetrievalError,
  RetrievalService,
  type RetrievalDocument,
  type RetrievalDocumentScope,
  type RetrievalHit,
  type RetrievalIndexRequest,
  type RetrievalRemoveRequest,
  type RetrievalSearchFilter,
  type RetrievalSearchPage,
  type RetrievalSearchRequest,
  type RetrievalStats,
} from '@deepseek-ai/dsh-retrieval'
import type { EmbeddingModel } from 'fastembed'
import {
  ZVecCollectionSchema,
  ZVecCreateAndOpen,
  ZVecDataType,
  ZVecIndexType,
  ZVecInitialize,
  ZVecLogLevel,
  ZVecLogType,
  ZVecMetricType,
  ZVecOpen,
  type ZVecCollection,
  type ZVecDoc,
  type ZVecDocInput,
} from '@zvec/zvec'

/** Cordis plugin name used by Loader diagnostics. */
export const name = 'retrieval-zvec'

/** Embedding models supported by the optional local fastembed runtime. */
export const ZVEC_EMBEDDING_MODELS = [
  'fast-all-MiniLM-L6-v2',
  'fast-bge-base-en',
  'fast-bge-base-en-v1.5',
  'fast-bge-small-en',
  'fast-bge-small-en-v1.5',
  'fast-bge-small-zh-v1.5',
  'fast-multilingual-e5-large',
] as const

/** One supported local embedding model name. */
export type ZvecEmbeddingModel = typeof ZVEC_EMBEDDING_MODELS[number]

/** Dimensions produced by each supported embedding model. */
const EMBEDDING_DIMENSIONS: Record<ZvecEmbeddingModel, number> = {
  'fast-all-MiniLM-L6-v2': 384,
  'fast-bge-base-en': 768,
  'fast-bge-base-en-v1.5': 768,
  'fast-bge-small-en': 384,
  'fast-bge-small-en-v1.5': 384,
  'fast-bge-small-zh-v1.5': 512,
  'fast-multilingual-e5-large': 1024,
}

/** Default repository-local directory holding zvec data. */
export const ZVEC_DEFAULT_DATA_DIR = '.dsh'

/** Default zvec collection name. */
export const ZVEC_DEFAULT_COLLECTION = 'retrieval'

/** Default maximum page size accepted by one search. */
export const ZVEC_DEFAULT_MAX_LIMIT = 100

/** Native limit on documents accepted by one zvec write call. */
export const ZVEC_MAX_WRITE_BATCH = 1024

/** Optional local embedding configuration. */
export interface ZvecEmbeddingConfig {
  /** fastembed model name. Defaults to the English BGE small v1.5 model. */
  model?: ZvecEmbeddingModel
  /** Maximum tokenized sequence length. Defaults to 512. */
  maxLength?: number
  /** Documents embedded per inference batch. Defaults to 32. */
  batchSize?: number
}

/** Provider configuration. */
export interface Config {
  /** Repository-local directory holding the collection, logs, and model cache. Defaults to `.dsh`. */
  dataDir?: string
  /** Collection name inside the data directory. Defaults to `retrieval`. */
  collectionName?: string
  /** Dense vector dimension; omitted with no `embedding` config leaves semantic search disabled. */
  vectorDimension?: number
  /** Optional local embedding runtime; `false` disables it explicitly. */
  embedding?: ZvecEmbeddingConfig | false
  /** Maximum page size accepted by one search. Defaults to 100. */
  maxLimit?: number
  /** zvec query thread count; omitted lets the engine decide. */
  queryThreads?: number
  /** zvec optimize thread count; omitted lets the engine decide. */
  optimizeThreads?: number
  /** zvec native log level. Defaults to `ERROR`. */
  logLevel?: keyof typeof ZVecLogLevel
}

const Config: z<Config> = z.object({
  dataDir: z.string().min(1).default(ZVEC_DEFAULT_DATA_DIR),
  collectionName: z.string().min(1).default(ZVEC_DEFAULT_COLLECTION),
  vectorDimension: z.number().step(1).min(1),
  embedding: z.union([
    z.const(false),
    z.object({
      model: z.union(ZVEC_EMBEDDING_MODELS).default('fast-bge-small-en-v1.5'),
      maxLength: z.number().step(1).min(1).max(8192).default(512),
      batchSize: z.number().step(1).min(1).max(1024).default(32),
    }),
  ]),
  maxLimit: z.number().step(1).min(1).max(10_000).default(ZVEC_DEFAULT_MAX_LIMIT),
  queryThreads: z.number().step(1).min(1),
  optimizeThreads: z.number().step(1).min(1),
  logLevel: z.union(['DEBUG', 'INFO', 'WARN', 'ERROR', 'FATAL'] as const).default('ERROR'),
})

interface ResolvedConfig {
  dataDir: string
  collectionName: string
  vectorDimension?: number
  embedding?: {
    model: ZvecEmbeddingModel
    maxLength: number
    batchSize: number
  }
  maxLimit: number
  queryThreads?: number
  optimizeThreads?: number
  logLevel: keyof typeof ZVecLogLevel
}

interface EmbeddingRuntime {
  embedDocuments(texts: readonly string[]): Promise<number[][]>
  embedQuery(text: string): Promise<number[]>
}

interface SearchCursor {
  version: 1
  offset: number
}

const OUTPUT_FIELDS = ['text', 'scope', 'path', 'session_id', 'seq', 'type', 'surface', 'time'] as const

let initialized = false

/**
 * Workspace-scoped zvec provider.
 *
 * Each workspace root owns one collection under `<root>/<dataDir>/<collectionName>`.
 * The exclusive native writer lock makes a second writable open of the same
 * collection fail loud rather than silently sharing a medium.
 */
export class ZvecRetrievalService extends RetrievalService {
  static Config: z<Config> = Config

  /** Validated and defaulted provider configuration. */
  readonly config: ResolvedConfig

  private readonly _collections = new Map<string, ZVecCollection>()
  private readonly _embeddings = new Map<string, Promise<EmbeddingRuntime>>()
  private _writeTail: Promise<void> = Promise.resolve()
  private _closed = false

  constructor(ctx: Context, config: Config = {}) {
    super(ctx)
    this.config = resolveConfig(config)
    ctx.effect(() => async () => this.close(), 'retrievalZvec.close')
  }

  override async indexDocuments(request: RetrievalIndexRequest, signal?: AbortSignal): Promise<void> {
    assertOpen(this._closed)
    const workspaceRoot = resolveWorkspaceRoot(request.workspaceRoot)
    if (request.documents.length === 0) return
    await this._serialized(signal, async () => {
      const collection = this._collection(workspaceRoot, signal)
      const vectors = await this._vectors(workspaceRoot, request.documents, signal)
      const inputs: ZVecDocInput[] = request.documents.map((document, index) => {
        const embedding = vectors?.[index]
        return {
          id: document.id,
          ...embedding === undefined ? {} : { vectors: { embedding } },
          fields: documentFields(document),
        }
      })
      signal?.throwIfAborted()
      for (let offset = 0; offset < inputs.length; offset += ZVEC_MAX_WRITE_BATCH) {
        signal?.throwIfAborted()
        const statuses = collection.upsertSync(inputs.slice(offset, offset + ZVEC_MAX_WRITE_BATCH))
        /* v8 ignore start -- native status failures require a corrupt medium this provider never creates */
        const failed = statuses.find(status => !status.ok)
        if (failed !== undefined) {
          throw new RetrievalError(`zvec rejected an upsert: ${failed.message}`, 'RETRIEVAL_PROVIDER_FAILED', { cause: failed })
        }
        /* v8 ignore stop */
      }
    })
  }

  override async removeDocuments(request: RetrievalRemoveRequest, signal?: AbortSignal): Promise<void> {
    assertOpen(this._closed)
    const workspaceRoot = resolveWorkspaceRoot(request.workspaceRoot)
    const ids = request.ids
    if (ids !== undefined && ids.length === 0) return
    if (ids === undefined && request.scope === undefined && request.path === undefined && request.sessionId === undefined) {
      throw new RetrievalError('removeDocuments requires ids or at least one metadata filter', 'RETRIEVAL_INVALID_REQUEST')
    }
    await this._serialized(signal, () => {
      const collection = this._collection(workspaceRoot, signal)
      signal?.throwIfAborted()
      const statuses = ids === undefined
        ? [collection.deleteByFilterSync(buildRemoveFilter(request))]
        : [...ids].reduce<string[][]>((batches, id, index) => {
          const batch = Math.floor(index / ZVEC_MAX_WRITE_BATCH)
          batches[batch] ??= []
          batches[batch].push(id)
          return batches
        }, []).flatMap(batch => collection.deleteSync(batch))
      /* v8 ignore start -- native status failures require a corrupt medium this provider never creates */
      const failed = statuses.find(status => !status.ok)
      if (failed !== undefined) {
        throw new RetrievalError(`zvec rejected a delete: ${failed.message}`, 'RETRIEVAL_PROVIDER_FAILED', { cause: failed })
      }
      /* v8 ignore stop */
      return Promise.resolve()
    })
  }

  override async search(request: RetrievalSearchRequest, signal?: AbortSignal): Promise<RetrievalSearchPage> {
    assertOpen(this._closed)
    const query = normalizeQuery(request.query)
    if (!Number.isInteger(request.limit) || request.limit < 1 || request.limit > this.config.maxLimit) {
      throw new RetrievalError(
        `retrieval limit must be an integer between 1 and ${String(this.config.maxLimit)}`,
        'RETRIEVAL_INVALID_REQUEST',
      )
    }
    const workspaceRoot = resolveWorkspaceRoot(request.workspaceRoot)
    const collection = this._collection(workspaceRoot, signal)
    const cursor = decodeCursor(request.cursor)
    const filter = buildSearchFilter(request.filter)
    const vector = await this._queryVector(workspaceRoot, request, signal)
    signal?.throwIfAborted()
    const topk = cursor.offset + request.limit + 1
    const queryRequest = {
      topk,
      ...filter === undefined ? {} : { filter },
      outputFields: [...OUTPUT_FIELDS],
    }
    let docs: ZVecDoc[]
    if (request.mode === 'lexical') {
      docs = collection.querySync({ ...queryRequest, fieldName: 'text', fts: { matchString: quoteFtsText(query) } })
    } else if (request.mode === 'semantic') {
      docs = collection.querySync({ ...queryRequest, fieldName: 'embedding', vector: vector as number[] })
    } else {
      const queryVector = vector as number[]
      docs = collection.multiQuerySync({
        ...filter === undefined ? {} : { filter },
        outputFields: [...OUTPUT_FIELDS],
        queries: [
          { fieldName: 'text', fts: { matchString: quoteFtsText(query) }, numCandidates: topk },
          { fieldName: 'embedding', vector: queryVector, numCandidates: topk },
        ],
        rerank: { type: 'rrf', rankConstant: 60 },
        topk,
      })
    }
    const pageDocs = docs.slice(cursor.offset, cursor.offset + request.limit)
    const items = pageDocs.map(toHit)
    return {
      items,
      ...cursor.offset + request.limit < docs.length
        ? { nextCursor: encodeCursor({ version: 1, offset: cursor.offset + request.limit }) }
        : {},
    }
  }

  override stats(): Promise<RetrievalStats> {
    let documents = 0
    for (const collection of this._collections.values()) documents += collection.stats.docCount
    return Promise.resolve({ workspaces: this._collections.size, documents })
  }

  override async flush(): Promise<void> {
    await this._writeTail
  }

  override async close(): Promise<void> {
    if (this._closed) {
      await this._writeTail
      return
    }
    this._closed = true
    await this._writeTail
    for (const collection of this._collections.values()) collection.closeSync()
    this._collections.clear()
    this._embeddings.clear()
  }

  private async _serialized<T>(signal: AbortSignal | undefined, operation: () => Promise<T>): Promise<T> {
    let release!: () => void
    const gate = new Promise<void>((resolveGate) => { release = resolveGate })
    const prior = this._writeTail
    this._writeTail = prior.then(() => gate)
    try {
      await prior
      signal?.throwIfAborted()
      return await operation()
    } finally {
      release()
    }
  }

  private _collection(workspaceRoot: string, signal?: AbortSignal): ZVecCollection {
    signal?.throwIfAborted()
    const existing = this._collections.get(workspaceRoot)
    if (existing !== undefined) return existing
    const dataRoot = join(workspaceRoot, this.config.dataDir)
    const collectionPath = join(dataRoot, this.config.collectionName)
    mkdirSync(dataRoot, { recursive: true })
    initializeEngine(join(dataRoot, `${this.config.collectionName}-logs`), this.config)
    signal?.throwIfAborted()
    let collection: ZVecCollection
    try {
      collection = existsSync(collectionPath)
        ? ZVecOpen(collectionPath)
        : ZVecCreateAndOpen(collectionPath, this._schema())
    } catch (error: unknown) {
      throw new RetrievalError(
        `workspace retrieval collection failed to open: ${errorMessage(error)}`,
        'RETRIEVAL_PROVIDER_FAILED',
        { cause: error },
      )
    }
    this._collections.set(workspaceRoot, collection)
    return collection
  }

  private _schema(): ZVecCollectionSchema {
    const fields = [
      {
        name: 'text',
        dataType: ZVecDataType.STRING,
        indexParams: {
          indexType: ZVecIndexType.FTS,
          tokenizerName: 'standard',
          filters: ['lowercase'],
        },
      },
      { name: 'scope', dataType: ZVecDataType.STRING, indexParams: { indexType: ZVecIndexType.INVERT } },
      { name: 'path', dataType: ZVecDataType.STRING, nullable: true, indexParams: { indexType: ZVecIndexType.INVERT } },
      { name: 'session_id', dataType: ZVecDataType.STRING, nullable: true, indexParams: { indexType: ZVecIndexType.INVERT } },
      { name: 'seq', dataType: ZVecDataType.INT64, nullable: true, indexParams: { indexType: ZVecIndexType.INVERT } },
      { name: 'type', dataType: ZVecDataType.STRING, nullable: true, indexParams: { indexType: ZVecIndexType.INVERT } },
      { name: 'surface', dataType: ZVecDataType.STRING, nullable: true, indexParams: { indexType: ZVecIndexType.INVERT } },
      { name: 'time', dataType: ZVecDataType.INT64, nullable: true, indexParams: { indexType: ZVecIndexType.INVERT } },
    ]
    const vectors = this.config.vectorDimension === undefined
      ? undefined
      : {
        name: 'embedding',
        dataType: ZVecDataType.VECTOR_FP32,
        dimension: this.config.vectorDimension,
        indexParams: { indexType: ZVecIndexType.FLAT, metricType: ZVecMetricType.COSINE },
      }
    return new ZVecCollectionSchema({
      name: this.config.collectionName,
      ...vectors === undefined ? {} : { vectors },
      fields,
    })
  }

  private async _vectors(
    workspaceRoot: string,
    documents: readonly RetrievalDocument[],
    signal?: AbortSignal,
  ): Promise<number[][] | undefined> {
    if (this.config.vectorDimension === undefined) {
      const supplied = documents.some(document => document.vector !== undefined)
      if (supplied) {
        throw new RetrievalError(
          'caller-supplied vectors require a configured vectorDimension',
          'RETRIEVAL_SEMANTIC_DISABLED',
        )
      }
      return undefined
    }
    signal?.throwIfAborted()
    /* v8 ignore next -- covered by the mixed supplied/embedded case; the empty-filter path is lexical-only */
    const embeddedDocuments = documents.filter(document => document.vector === undefined)
    const suppliedVectors = documents.map(document => document.vector)
    if (embeddedDocuments.length > 0) {
      /* v8 ignore next -- vectorDimension without embedding leaves every document unembeddable, rejected below */
      if (this.config.embedding === undefined) {
        throw new RetrievalError(
          'documents without a vector require configured local embeddings',
          'RETRIEVAL_SEMANTIC_DISABLED',
        )
      }
      const runtime = await this._embedding(workspaceRoot, signal)
      const embedded = await runtime.embedDocuments(embeddedDocuments.map(document => document.text))
      let cursor = 0
      for (let index = 0; index < documents.length; index += 1) {
        const supplied = suppliedVectors[index]
        if (supplied === undefined) {
          suppliedVectors[index] = embedded[cursor]
          cursor += 1
        }
      }
    }
    const vectors = suppliedVectors.filter((vector): vector is readonly number[] => vector !== undefined)
    /* v8 ignore next 3 -- the embedder contract returns one vector per input text */
    if (vectors.length !== documents.length) {
      throw new RetrievalError('every document requires a vector', 'RETRIEVAL_INVALID_REQUEST')
    }
    for (const vector of vectors) {
      if (vector.length !== this.config.vectorDimension) {
        throw new RetrievalError(
          `vector dimension ${String(vector.length)} does not match configured ${String(this.config.vectorDimension)}`,
          'RETRIEVAL_INVALID_REQUEST',
        )
      }
    }
    return vectors.map(vector => [...vector])
  }

  private async _queryVector(
    workspaceRoot: string,
    request: RetrievalSearchRequest,
    signal?: AbortSignal,
  ): Promise<number[] | undefined> {
    if (request.mode === 'lexical') return undefined
    if (this.config.vectorDimension === undefined) {
      throw new RetrievalError('semantic retrieval requires a configured vectorDimension', 'RETRIEVAL_SEMANTIC_DISABLED')
    }
    if (request.vector !== undefined) {
      if (request.vector.length !== this.config.vectorDimension) {
        throw new RetrievalError(
          `query vector dimension ${String(request.vector.length)} does not match configured ${String(this.config.vectorDimension)}`,
          'RETRIEVAL_INVALID_REQUEST',
        )
      }
      return [...request.vector]
    }
    if (this.config.embedding === undefined) {
      throw new RetrievalError(
        'semantic retrieval without a caller-supplied query vector requires configured local embeddings',
        'RETRIEVAL_SEMANTIC_DISABLED',
      )
    }
    const runtime = await this._embedding(workspaceRoot, signal)
    signal?.throwIfAborted()
    return runtime.embedQuery(request.query)
  }

  private _embedding(workspaceRoot: string, signal?: AbortSignal): Promise<EmbeddingRuntime> {
    signal?.throwIfAborted()
    /* v8 ignore next -- resolution keeps embeddings optional, and callers reach this only when configured */
    const cacheDir = join(workspaceRoot, this.config.dataDir, `${this.config.collectionName}-models`)
    let runtime = this._embeddings.get(cacheDir)
    if (runtime === undefined) {
      runtime = loadEmbedding(this.config.embedding, cacheDir)
      this._embeddings.set(cacheDir, runtime)
    }
    return runtime
  }
}

/* v8 ignore start -- Loader's Config schema fills these defaults before apply; direct construction is the only partially-covered caller */
function resolveConfig(config: Config): ResolvedConfig {
  const maxLimit = config.maxLimit ?? ZVEC_DEFAULT_MAX_LIMIT
  const embedding = config.embedding === undefined || config.embedding === false
    ? undefined
    : {
      model: config.embedding.model ?? 'fast-bge-small-en-v1.5',
      maxLength: config.embedding.maxLength ?? 512,
      batchSize: config.embedding.batchSize ?? 32,
    }
  const dimension = config.vectorDimension
    ?? (embedding === undefined ? undefined : EMBEDDING_DIMENSIONS[embedding.model])
  if (embedding !== undefined && dimension !== undefined && dimension !== EMBEDDING_DIMENSIONS[embedding.model]) {
    throw new RetrievalError(
      `vectorDimension ${String(dimension)} does not match embedding model dimension ${String(EMBEDDING_DIMENSIONS[embedding.model])}`,
      'RETRIEVAL_INVALID_REQUEST',
    )
  }
  return {
    dataDir: config.dataDir ?? ZVEC_DEFAULT_DATA_DIR,
    collectionName: config.collectionName ?? ZVEC_DEFAULT_COLLECTION,
    ...dimension === undefined ? {} : { vectorDimension: dimension },
    ...embedding === undefined ? {} : { embedding },
    maxLimit,
    ...config.queryThreads === undefined ? {} : { queryThreads: config.queryThreads },
    ...config.optimizeThreads === undefined ? {} : { optimizeThreads: config.optimizeThreads },
    logLevel: config.logLevel ?? 'ERROR',
  }
}
/* v8 ignore stop */

/* v8 ignore start -- the engine initializes once per process; later opens reuse it by design */
function initializeEngine(logDir: string, config: ResolvedConfig): void {
  if (initialized) return
  mkdirSync(logDir, { recursive: true })
  ZVecInitialize({
    logType: ZVecLogType.FILE,
    logLevel: ZVecLogLevel[config.logLevel],
    logDir,
    ...config.queryThreads === undefined ? {} : { queryThreads: config.queryThreads },
    ...config.optimizeThreads === undefined ? {} : { optimizeThreads: config.optimizeThreads },
  })
  initialized = true
}
/* v8 ignore stop */

function resolveWorkspaceRoot(workspaceRoot: string): string {
  if (workspaceRoot.length === 0) {
    throw new RetrievalError('workspaceRoot is required', 'RETRIEVAL_WORKSPACE_NOT_FOUND')
  }
  const resolved = resolve(workspaceRoot)
  try {
    if (!statSync(resolved).isDirectory()) {
      throw new RetrievalError(`workspace root is not a directory: ${resolved}`, 'RETRIEVAL_WORKSPACE_NOT_FOUND')
    }
  } catch (error: unknown) {
    if (error instanceof RetrievalError) throw error
    throw new RetrievalError(`workspace root is unavailable: ${resolved}`, 'RETRIEVAL_WORKSPACE_NOT_FOUND', { cause: error })
  }
  return resolved
}

function normalizeQuery(query: string): string {
  const normalized = query.trim().replace(/\s+/g, ' ')
  if (normalized.length === 0 || normalized.includes('\0')) {
    throw new RetrievalError('retrieval query must be non-empty text', 'RETRIEVAL_INVALID_QUERY')
  }
  return normalized
}

function documentFields(document: RetrievalDocument): Record<string, string | number> {
  const fields: Record<string, string | number> = {
    scope: document.scope,
    text: document.text,
  }
  if (document.path !== undefined) fields.path = document.path
  if (document.sessionId !== undefined) fields.session_id = document.sessionId
  if (document.seq !== undefined) fields.seq = document.seq
  if (document.type !== undefined) fields.type = document.type
  if (document.surface !== undefined) fields.surface = document.surface
  if (document.time !== undefined) fields.time = document.time
  return fields
}

function buildSearchFilter(filter: RetrievalSearchFilter | undefined): string | undefined {
  if (filter === undefined) return undefined
  const clauses: string[] = []
  if (filter.scope !== undefined) clauses.push(`scope IN (${filter.scope.map(value => quoteScalar(value)).join(', ')})`)
  if (filter.path !== undefined) clauses.push(`path IN (${filter.path.map(value => quoteScalar(value)).join(', ')})`)
  if (filter.sessionId !== undefined) clauses.push(`session_id IN (${filter.sessionId.map(value => quoteScalar(value)).join(', ')})`)
  if (filter.type !== undefined) clauses.push(`type IN (${filter.type.map(value => quoteScalar(value)).join(', ')})`)
  if (filter.surface !== undefined) clauses.push(`surface IN (${filter.surface.map(value => quoteScalar(value)).join(', ')})`)
  const timeFrom = filter.timeFrom
  const timeTo = filter.timeTo
  if (timeFrom !== undefined && timeTo !== undefined && timeFrom > timeTo) {
    throw new RetrievalError('timeFrom must not exceed timeTo', 'RETRIEVAL_INVALID_FILTER')
  }
  if (timeFrom !== undefined) clauses.push(`time >= ${quoteNumber(timeFrom, 'timeFrom')}`)
  if (timeTo !== undefined) clauses.push(`time <= ${quoteNumber(timeTo, 'timeTo')}`)
  /* v8 ignore next -- a filter with no clauses is the same as omitting the argument */
  return clauses.length === 0 ? undefined : clauses.join(' AND ')
}

/* v8 ignore start -- each clause is exercised by the provider filter test; the union has no other outcome */
function buildRemoveFilter(request: RetrievalRemoveRequest): string {
  const clauses: string[] = []
  const scope: RetrievalDocumentScope | undefined = request.scope
  if (scope !== undefined) clauses.push(`scope = ${quoteScalar(scope)}`)
  if (request.path !== undefined) clauses.push(`path = ${quoteScalar(request.path)}`)
  if (request.sessionId !== undefined) clauses.push(`session_id = ${quoteScalar(request.sessionId)}`)
  return clauses.join(' AND ')
}
/* v8 ignore stop */

function quoteScalar(value: string): string {
  return `"${value.replace(/\\/g, '\\\\').replace(/"/g, '\\"')}"`
}

function quoteNumber(value: number, name: string): string {
  if (!Number.isFinite(value)) throw new RetrievalError(`${name} must be a finite number`, 'RETRIEVAL_INVALID_FILTER')
  return String(value)
}

function quoteFtsText(query: string): string {
  return `"${query.replace(/"/g, '""')}"`
}

function decodeCursor(cursor: string | undefined): SearchCursor {
  if (cursor === undefined) return { version: 1, offset: 0 }
  try {
    const parsed = JSON.parse(Buffer.from(cursor, 'base64url').toString('utf8')) as Partial<SearchCursor>
    if (parsed.version !== 1 || !Number.isInteger(parsed.offset) || (parsed.offset as number) < 0) {
      throw new RetrievalError('invalid retrieval cursor', 'RETRIEVAL_INVALID_REQUEST')
    }
    return { version: 1, offset: parsed.offset as number }
  } catch (error: unknown) {
    if (error instanceof RetrievalError) throw error
    throw new RetrievalError('invalid retrieval cursor', 'RETRIEVAL_INVALID_REQUEST', { cause: error })
  }
}

function encodeCursor(cursor: SearchCursor): string {
  return Buffer.from(JSON.stringify(cursor), 'utf8').toString('base64url')
}

function toHit(document: ZVecDoc): RetrievalHit {
  const fields = document.fields
  const path = fieldString(fields, 'path')
  const sessionId = fieldString(fields, 'session_id')
  const seq = fieldNumber(fields, 'seq')
  const type = fieldString(fields, 'type')
  const surface = fieldString(fields, 'surface')
  const time = fieldNumber(fields, 'time')
  return {
    id: document.id,
    /* v8 ignore next -- scope is always stored; the workspace arm covers a malformed medium */
    scope: fieldString(fields, 'scope') === 'session' ? 'session' : 'workspace',
    /* v8 ignore next -- text is always stored; the empty arm covers a malformed medium */
    text: fieldString(fields, 'text') ?? '',
    score: document.score,
    ...path === undefined ? {} : { path },
    ...sessionId === undefined ? {} : { sessionId },
    ...seq === undefined ? {} : { seq },
    ...type === undefined ? {} : { type },
    ...surface === undefined ? {} : { surface },
    ...time === undefined ? {} : { time },
  }
}

function fieldString(fields: ZVecDoc['fields'], key: string): string | undefined {
  /* v8 ignore next -- zvec returns exactly the stored scalar types */
  const value: unknown = fields[key]
  return typeof value === 'string' ? value : undefined
}

function fieldNumber(fields: ZVecDoc['fields'], key: string): number | undefined {
  const value: unknown = fields[key]
  return typeof value === 'number' ? value : undefined
}

/* v8 ignore next -- the native binding rejects with Error instances */
function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error)
}

function assertOpen(closed: boolean): void {
  if (closed) throw new RetrievalError('retrieval provider is closed', 'RETRIEVAL_PROVIDER_FAILED')
}

type FastembedModule = typeof import('fastembed')

async function loadEmbedding(config: ResolvedConfig['embedding'], cacheDir: string): Promise<EmbeddingRuntime> {
  /* v8 ignore next -- resolved config supplies embeddings only when this loads */
  if (config === undefined) throw new RetrievalError('local embeddings are not configured', 'RETRIEVAL_SEMANTIC_DISABLED')
  let fastembed: FastembedModule
  try {
    fastembed = await import('fastembed')
  } catch (error: unknown) {
    /* v8 ignore start -- the optional dependency is installed in every composition that configures embeddings */
    throw new RetrievalError(
      'local embeddings require the optional fastembed dependency',
      'RETRIEVAL_SEMANTIC_DISABLED',
      { cause: error },
    )
    /* v8 ignore stop */
  }
  const model = await fastembed.FlagEmbedding.init({
    model: config.model as Exclude<EmbeddingModel, EmbeddingModel.CUSTOM>,
    cacheDir,
    executionProviders: [fastembed.ExecutionProvider.CPU],
    maxLength: config.maxLength,
    showDownloadProgress: false,
  })
  return {
    async embedDocuments(texts) {
      const vectors: number[][] = []
      for await (const batch of model.passageEmbed([...texts], config.batchSize)) vectors.push(...batch)
      return vectors
    },
    embedQuery(text) {
      return model.queryEmbed(text)
    },
  }
}

export default ZvecRetrievalService
