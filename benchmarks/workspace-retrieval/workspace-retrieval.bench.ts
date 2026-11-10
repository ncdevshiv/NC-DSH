/**
 * Workspace retrieval performance, retrieval quality, and token-cost gate.
 *
 * Compares the shipped zvec provider against in-process lexical and
 * brute-force vector references over one fixed synthetic corpus. Quality and
 * token rows are ratios against the reference; timings use the shared
 * benchmark calibration and reporting helpers.
 */

import { describe, expect, it } from 'vitest'
import { join } from 'node:path'
import { runBuiltBenchmarkWorker } from '../support/built-worker.ts'
import { ciTimeBudget } from '../support/calibration.ts'
import { recordTimings, type BenchmarkCase } from '../support/scaling-report.ts'
import {
  buildCorpus,
  buildQueries,
  CORPUS_DOCUMENTS,
  VECTOR_DIMENSION,
} from './corpus.ts'
import type { WorkspaceRetrievalReport } from './workspace-retrieval.worker.ts'

const WORKER = join(import.meta.dirname, '..', '.dsh-build', 'workspace-retrieval', 'workspace-retrieval.worker.js')
const WORKER_TIMEOUT_MS = 180_000

/** Reference-machine budgets; none of these endpoints existed before this provider. */
const BUDGET_MS = {
  index: ciTimeBudget(2_000),
  lexical: ciTimeBudget(15),
  semantic: ciTimeBudget(15),
  hybrid: ciTimeBudget(25),
  close: ciTimeBudget(160),
} as const

const CASE: BenchmarkCase = {
  id: 'workspace-retrieval/zvec-vs-reference',
  measures: `Index ${CORPUS_DOCUMENTS.toLocaleString('en-US')} synthetic documents, then lexical, semantic, and hybrid search, compared with in-process lexical and brute-force vector references over the identical corpus and queries.`,
  affects: 'Model workspace_search calls and workspace_index writes; a lexical-only retrieval deployment pays the default index and search path.',
}

describe('workspace retrieval', () => {
  it('indexes and searches with comparable quality and bounded cost', async () => {
    const corpus = buildCorpus()
    const queries = buildQueries()
    expect(corpus).toHaveLength(CORPUS_DOCUMENTS)
    expect(queries.length).toBeGreaterThan(0)
    expect(corpus.every(document => document.text.length > 0)).toBe(true)
    expect(Number.isInteger(VECTOR_DIMENSION)).toBe(true)

    const run = await runBuiltBenchmarkWorker<WorkspaceRetrievalReport>({
      worker: WORKER,
      timeoutMs: WORKER_TIMEOUT_MS,
    })
    expect(run.stderr).toBe('')
    expect(run.timedOut).toBe(false)
    expect(run.signal).toBeNull()
    expect(run.exitCode).toBe(0)
    const report = run.report
    if (report === undefined) throw new Error('workspace-retrieval worker did not report a result')
    expect(report).toMatchObject({
      mode: 'measure',
      documents: CORPUS_DOCUMENTS,
      topK: 10,
    })
    expect(report.lexicalQueries).toBeGreaterThan(0)
    expect(report.semanticQueries).toBeGreaterThan(0)

    // Quality ratios: zvec's lexical FTS must find the same planted documents
  // as the exact reference; semantic FLAT vectors must equal brute force.
    expect(report.zvecLexical.recall).toBeGreaterThanOrEqual(report.lexicalReference.recall)
    expect(report.zvecLexical.precision).toBeGreaterThanOrEqual(report.lexicalReference.precision)
    expect(report.zvecSemantic.recall).toBeGreaterThanOrEqual(report.semanticReference.recall * 0.99)
    expect(report.zvecSemantic.precision).toBeGreaterThanOrEqual(report.semanticReference.precision * 0.99)
    expect(report.zvecHybrid.recall).toBeGreaterThan(0)
    expect(report.tokens.zvecPerQuery).toBeLessThanOrEqual(report.tokens.referencePerQuery * 1.2 + 8)

    const median = (values: readonly number[]): number => {
      const sorted = [...values].sort((left, right) => left - right)
      return sorted[Math.floor(sorted.length / 2)] ?? 0
    }
    const timings = {
      bootMs: { ms: report.bootMs, cpuMs: report.bootMs },
      indexMs: { ms: report.indexMs, cpuMs: report.indexMs },
      lexicalMs: { ms: median(report.lexicalMs.map(entry => entry.ms)), cpuMs: median(report.lexicalMs.map(entry => entry.cpuMs)) },
      semanticMs: { ms: median(report.semanticMs.map(entry => entry.ms)), cpuMs: median(report.semanticMs.map(entry => entry.cpuMs)) },
      hybridMs: { ms: median(report.hybridMs.map(entry => entry.ms)), cpuMs: median(report.hybridMs.map(entry => entry.cpuMs)) },
      closeMs: { ms: report.closeMs, cpuMs: report.closeMs },
    }
    recordTimings(CASE, timings, BUDGET_MS)
    expect(timings.indexMs.ms).toBeLessThanOrEqual(BUDGET_MS.index)
    expect(timings.lexicalMs.ms).toBeLessThanOrEqual(BUDGET_MS.lexical)
    expect(timings.semanticMs.ms).toBeLessThanOrEqual(BUDGET_MS.semantic)
    expect(timings.hybridMs.ms).toBeLessThanOrEqual(BUDGET_MS.hybrid)
    expect(timings.closeMs.ms).toBeLessThanOrEqual(BUDGET_MS.close)
  })
})
