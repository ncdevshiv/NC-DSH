# 检索

[English](retrieval.md) | 中文

按工作区划分的检索，覆盖文件与会话事件。[服务定义包](../../packages/retrieval/retrieval)拥有请求与结果词汇、排序模式、过滤与错误码；[zvec 提供方](../../packages/retrieval/retrieval-zvec)拥有仓库本地集合、原生全文与向量搜索，以及可选本地嵌入。

来源：[`packages/retrieval/retrieval/src/index.ts`](../../packages/retrieval/retrieval/src/index.ts) · [`packages/retrieval/retrieval-zvec/src/index.ts`](../../packages/retrieval/retrieval-zvec/src/index.ts)

## 服务：`ctx.retrieval`

`RetrievalService` 是一个按工作区划分的接缝。每个请求都指定绝对工作区根目录，因此一个提供方实例可服务多个项目，而不依赖进程级“当前工作区”状态。提供方拥有介质；消费者从不直接访问特定引擎。

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

## 记录

`RetrievalDocument` 是输入记录。`RetrievalHit` 是输出记录；它保留来源范围以及可选的路径或会话元数据。只有部署配置了向量维度时，稠密向量才会出现在这两类记录上。

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

`RetrievalSearchFilter` 对提供的字段做 AND：范围、路径、会话 id、事件类型、事件表面以及闭区间时间范围。同一字段内的数组为 OR。

## 模式

`lexical` 把查询作为一个字面短语引用，并使用原生全文索引排序。`semantic` 需要已配置的向量维度，以及调用方查询向量或已配置的本地嵌入器。`hybrid` 运行一个全文子查询和一个向量子查询，并用倒数排名融合（排名常数 60）合并。没有向量维度的部署会以 `RETRIEVAL_SEMANTIC_DISABLED` 拒绝语义与混合调用，而不是静默回退。

## 仓库本地存储

随发行版提供方为每个集合、原生日志目录和嵌入模型缓存解析 `<workspace>/<dataDir>`；`dataDir` 默认为 `.dsh`。同一集合的第二次可写打开会失败，因为 zvec 持有原生独占锁，所以同一时间只有一个 DSH 进程拥有某工作区集合。提供方绝不回退到进程工作目录。

<!-- BEGIN GENERATED cordis-surface (gen-cordis-catalog.ts) — do not edit between markers -->

<a id="cordis-surface"></a>

## Cordis API

Generated from source by `scripts/gen-cordis-catalog.ts` (verified fresh by `bun run verify-cordis-catalog` in doc-sync; regenerate with `bun run gen-cordis-catalog`) — the language sides differ only in locale-specific paired document paths. Signature blocks use a `ts cordis-catalog` fence and keep the original source JSDoc; dispatch modes are defined in the [primer](../cordis-primer.zh.md#dispatch-modes), and the framework-inherited `ctx` API lives in [cordis-api/inherited.md](../cordis-api/inherited.md).

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
