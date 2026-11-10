/**
 * Plain-Node worker for the workspace retrieval benchmark. It measures the
 * shipped zvec provider against in-process lexical and brute-force vector
 * references over the identical synthetic corpus, then reports retrieval
 * quality, token cost, and timing. Run built.
 */

import { mkdtemp, rm } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { performance } from 'node:perf_hooks'
import { Context } from '@deepseek-ai/cordis'
import { estimateContent } from '@deepseek-ai/dsh-token-meter/estimate'
import ZvecRetrievalService from '@deepseek-ai/dsh-retrieval-zvec'
import { assertBuiltBenchmarkRuntime } from '../support/built-worker.ts'
import {
  buildCorpus,
  buildQueries,
  CORPUS_DOCUMENTS,
  TOP_K,
  VECTOR_DIMENSION,
  type SyntheticRetrievalDocument,
  type SyntheticRetrievalQuery,
} from './corpus.ts'

/** Quality of one mode's ranked result set. */
export interface QualityReport {
  /** Fraction of relevant documents found within the top K. */
  readonly recall: number
  /** Fraction of returned documents that are relevant. */
  readonly precision: number
  /** Average reciprocal rank of the first relevant hit. */
  readonly meanReciprocalRank: number
  /** Fraction of queries with at least one relevant hit. */
  readonly hitRate: number
}

/** Wall time and process CPU time of one endpoint. */
export interface Endpoint {
  /** Wall-clock milliseconds. */
  readonly ms: number
  /** Process CPU milliseconds over the same interval. */
  readonly cpuMs: number
}

/** Final JSON line printed by the worker. */
export interface WorkspaceRetrievalReport {
  readonly mode: 'measure'
  readonly documents: number
  readonly lexicalQueries: number
  readonly semanticQueries: number
  readonly topK: number
  readonly lexicalReference: QualityReport
  readonly zvecLexical: QualityReport
  readonly semanticReference: QualityReport
  readonly zvecSemantic: QualityReport
  readonly hybridReference: QualityReport
  readonly zvecHybrid: QualityReport
  readonly tokens: {
    readonly referencePerQuery: number
    readonly zvecPerQuery: number
  }
  readonly lexicalMs: readonly Endpoint[]
  readonly semanticMs: readonly Endpoint[]
  readonly hybridMs: readonly Endpoint[]
  readonly indexMs: number
  readonly bootMs: number
  readonly closeMs: number
}

function cpuNow(): number {
  const usage = process.cpuUsage()
  return (usage.user + usage.system) / 1000
}

function measureSync(run: () => void): Endpoint {
  const wall = performance.now()
  const cpu = cpuNow()
  run()
  return { ms: performance.now() - wall, cpuMs: cpuNow() - cpu }
}

function quality(hits: readonly string[], relevant: readonly string[]): QualityReport {
  const relevantSet = new Set(relevant)
  let found = 0
  let reciprocalRank = 0
  for (let rank = 0; rank < hits.length; rank += 1) {
    const id = hits[rank]
    if (id === undefined || !relevantSet.has(id)) continue
    found += 1
    if (reciprocalRank === 0) reciprocalRank = 1 / (rank + 1)
  }
  return {
    recall: found / Math.max(relevant.length, 1),
    precision: found / Math.max(hits.length, 1),
    meanReciprocalRank: reciprocalRank,
    hitRate: found > 0 ? 1 : 0,
  }
}

function average(reports: readonly QualityReport[]): QualityReport {
  const count = Math.max(reports.length, 1)
  return {
    recall: reports.reduce((sum, report) => sum + report.recall, 0) / count,
    precision: reports.reduce((sum, report) => sum + report.precision, 0) / count,
    meanReciprocalRank: reports.reduce((sum, report) => sum + report.meanReciprocalRank, 0) / count,
    hitRate: reports.reduce((sum, report) => sum + report.hitRate, 0) / count,
  }
}

function lexicalReference(
  documents: readonly SyntheticRetrievalDocument[],
  query: SyntheticRetrievalQuery,
  topK: number,
): readonly string[] {
  const term = query.lexicalTerm
  if (term === undefined) return []
  const matched: Array<{ id: string; score: number }> = []
  for (const document of documents) {
    if (!document.text.includes(term)) continue
    matched.push({ id: document.id, score: 1 })
  // Ties break by id so the reference and zvec rank the same document set.
  }
  matched.sort((left, right) => right.score - left.score || left.id.localeCompare(right.id))
  return matched.slice(0, topK).map(entry => entry.id)
}

function cosine(left: readonly number[], right: readonly number[]): number {
  let dot = 0
  let leftNorm = 0
  let rightNorm = 0
  for (let index = 0; index < left.length; index += 1) {
    const l = left[index] ?? 0
    const r = right[index] ?? 0
    dot += l * r
    leftNorm += l * l
    rightNorm += r * r
  }
  const denominator = Math.sqrt(leftNorm) * Math.sqrt(rightNorm)
  return denominator === 0 ? 0 : dot / denominator
}

function semanticReference(
  documents: readonly SyntheticRetrievalDocument[],
  query: SyntheticRetrievalQuery,
  topK: number,
): readonly string[] {
  const vector = query.vector
  if (vector === undefined) return []
  const scored: Array<{ id: string; score: number }> = []
  for (const document of documents) {
    scored.push({ id: document.id, score: cosine(document.vector, vector) })
  }
  scored.sort((left, right) => right.score - left.score || left.id.localeCompare(right.id))
  return scored.slice(0, topK).map(entry => entry.id)
}

/** Reciprocal-rank fusion over lexical and semantic reference lists. */
function hybridReference(
  documents: readonly SyntheticRetrievalDocument[],
  query: SyntheticRetrievalQuery,
  topK: number,
): readonly string[] {
  const lexical = lexicalReference(documents, query, topK)
  const semantic = semanticReference(documents, query, topK)
  const scores = new Map<string, number>()
  for (const [rank, id] of lexical.entries()) scores.set(id, (scores.get(id) ?? 0) + 1 / (60 + rank + 1))
  for (const [rank, id] of semantic.entries()) scores.set(id, (scores.get(id) ?? 0) + 1 / (60 + rank + 1))
  return [...scores.entries()]
    .sort(([leftId, leftScore], [rightId, rightScore]) => rightScore - leftScore || leftId.localeCompare(rightId))
    .slice(0, topK)
    .map(([id]) => id)
}

function textTokens(documents: ReadonlyMap<string, string>, ids: readonly string[]): number {
  return estimateContent(ids.map(id => ({ type: 'text' as const, text: (documents.get(id) ?? '').slice(0, 240) })))
}

async function main(): Promise<void> {
  assertBuiltBenchmarkRuntime(import.meta.url, {
    '@deepseek-ai/dsh-retrieval-zvec': import.meta.resolve('@deepseek-ai/dsh-retrieval-zvec'),
    '@deepseek-ai/dsh-retrieval': import.meta.resolve('@deepseek-ai/dsh-retrieval'),
  })
  const root = await mkdtemp(join(tmpdir(), 'dsh-workspace-retrieval-bench-'))
  try {
    const documents = buildCorpus()
    const queries = buildQueries()
    const textById = new Map(documents.map(document => [document.id, document.text]))

    const ctx = new Context()
    const bootedAt = performance.now()
    await ctx.plugin(ZvecRetrievalService, {
      embedding: false,
      dataDir: '.dsh',
      vectorDimension: VECTOR_DIMENSION,
    })
    const bootMs = performance.now() - bootedAt

    const indexEndpoint = await (async (): Promise<Endpoint> => {
      const wall = performance.now()
      const cpu = cpuNow()
      for (let offset = 0; offset < documents.length; offset += 200) {
        await ctx.retrieval.indexDocuments({
          workspaceRoot: root,
          documents: documents.slice(offset, offset + 200).map(document => ({
            id: document.id,
            scope: 'workspace' as const,
            text: document.text,
            path: document.path,
            vector: document.vector,
          })),
        })
      }
      await ctx.retrieval.flush()
      return { ms: performance.now() - wall, cpuMs: cpuNow() - cpu }
    })()

    const lexicalQueries = queries.filter(query => query.lexicalTerm !== undefined)
    const semanticQueries = queries.filter(query => query.lexicalTerm === undefined)

    const lexicalReferenceReports: QualityReport[] = []
    const zvecLexicalReports: QualityReport[] = []
    const lexicalTimings: Endpoint[] = []
    let referenceTokens = 0
    let zvecTokens = 0
    for (const query of lexicalQueries) {
      const timing = measureSync(() => {
        const ids = lexicalReference(documents, query, TOP_K)
        lexicalReferenceReports.push(quality(ids, query.relevant))
        referenceTokens += textTokens(textById, ids)
      })
      lexicalTimings.push(timing)
      const page = await ctx.retrieval.search({ workspaceRoot: root, query: query.text, mode: 'lexical', limit: TOP_K })
      const ids = page.items.map(item => item.id)
      zvecLexicalReports.push(quality(ids, query.relevant))
      zvecTokens += textTokens(textById, ids)
    }

    const semanticReferenceReports: QualityReport[] = []
    const zvecSemanticReports: QualityReport[] = []
    const semanticTimings: Endpoint[] = []
    let referenceSemanticTokens = 0
    let zvecSemanticTokens = 0
    for (const query of semanticQueries) {
      const timing = measureSync(() => {
        const ids = semanticReference(documents, query, TOP_K)
        semanticReferenceReports.push(quality(ids, query.relevant))
        referenceSemanticTokens += textTokens(textById, ids)
      })
      semanticTimings.push(timing)
      const page = await ctx.retrieval.search({
        workspaceRoot: root,
        query: query.text,
        mode: 'semantic',
        limit: TOP_K,
        ...query.vector === undefined ? {} : { vector: query.vector },
      })
      const ids = page.items.map(item => item.id)
      zvecSemanticReports.push(quality(ids, query.relevant))
      zvecSemanticTokens += textTokens(textById, ids)
    }

    const hybridReferenceReports: QualityReport[] = []
    const zvecHybridReports: QualityReport[] = []
    const hybridTimings: Endpoint[] = []
    for (const query of semanticQueries) {
      const timing = measureSync(() => {
        hybridReferenceReports.push(quality(hybridReference(documents, query, TOP_K), query.relevant))
      })
      hybridTimings.push(timing)
      const page = await ctx.retrieval.search({
        workspaceRoot: root,
        query: query.text,
        mode: 'hybrid',
        limit: TOP_K,
        ...query.vector === undefined ? {} : { vector: query.vector },
      })
      zvecHybridReports.push(quality(page.items.map(item => item.id), query.relevant))
    }

    const closeEndpoint = await (async (): Promise<Endpoint> => {
      const wall = performance.now()
      const cpu = cpuNow()
      await ctx.retrieval.close()
      return { ms: performance.now() - wall, cpuMs: cpuNow() - cpu }
    })()

    const report: WorkspaceRetrievalReport = {
      mode: 'measure',
      documents: CORPUS_DOCUMENTS,
      lexicalQueries: lexicalQueries.length,
      semanticQueries: semanticQueries.length,
      topK: TOP_K,
      lexicalReference: average(lexicalReferenceReports),
      zvecLexical: average(zvecLexicalReports),
      semanticReference: average(semanticReferenceReports),
      zvecSemantic: average(zvecSemanticReports),
      hybridReference: average(hybridReferenceReports),
      zvecHybrid: average(zvecHybridReports),
      tokens: {
        referencePerQuery: (referenceTokens + referenceSemanticTokens)
          / Math.max(lexicalQueries.length + semanticQueries.length, 1),
        zvecPerQuery: (zvecTokens + zvecSemanticTokens)
          / Math.max(lexicalQueries.length + semanticQueries.length, 1),
      },
      lexicalMs: lexicalTimings,
      semanticMs: semanticTimings,
      hybridMs: hybridTimings,
      indexMs: indexEndpoint.ms,
      bootMs,
      closeMs: closeEndpoint.ms,
    }
    process.stdout.write(`${JSON.stringify(report)}\n`)
  } finally {
    await rm(root, { recursive: true, force: true, maxRetries: 10, retryDelay: 200 })
  }
}

await main()
