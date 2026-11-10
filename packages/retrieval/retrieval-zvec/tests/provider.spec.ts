import { afterEach, describe, expect, it } from 'vitest'
import { mkdtemp, rm } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { Context } from '@deepseek-ai/cordis'
import RetrievalService, { RetrievalError } from '@deepseek-ai/dsh-retrieval'
import ZvecRetrievalService from '@deepseek-ai/dsh-retrieval-zvec'

const directories: string[] = []

afterEach(async () => {
  for (const directory of directories.splice(0)) await rm(directory, { recursive: true, force: true, maxRetries: 10, retryDelay: 200 })
})

async function workspace(): Promise<string> {
  const directory = await mkdtemp(join(tmpdir(), 'dsh-retrieval-zvec-'))
  directories.push(directory)
  return directory
}

async function mount(root: string, config: ConstructorParameters<typeof ZvecRetrievalService>[1] = {}): Promise<Context> {
  const ctx = new Context()
  await ctx.plugin(ZvecRetrievalService, { embedding: false, ...config, dataDir: '.dsh' })
  expect(ctx.retrieval).toBeInstanceOf(RetrievalService)
  expect(ctx.retrieval).toBeInstanceOf(ZvecRetrievalService)
  void root
  return ctx
}

describe('zvec retrieval provider', () => {
  it('indexes, searches lexically, paginates, and persists under the workspace data directory', async () => {
    const root = await workspace()
    const ctx = await mount(root)
    await ctx.retrieval.indexDocuments({
      workspaceRoot: root,
      documents: [
        { id: 'a', scope: 'workspace', text: 'alpha beta needle', path: 'src/a.ts' },
        { id: 'b', scope: 'workspace', text: 'beta gamma needle', path: 'src/b.ts' },
        { id: 'c', scope: 'session', text: 'delta needle', sessionId: 's', seq: 1, time: 5, type: 'user/message', surface: 'current' },
      ],
    })
    const first = await ctx.retrieval.search({ workspaceRoot: root, query: 'needle', mode: 'lexical', limit: 2 })
    expect(first.items).toHaveLength(2)
    expect(first.nextCursor).toBeDefined()
    const second = await ctx.retrieval.search({ workspaceRoot: root, query: 'needle', mode: 'lexical', limit: 2, ...first.nextCursor === undefined ? {} : { cursor: first.nextCursor } })
    expect(second.items).toHaveLength(1)
    expect(second.nextCursor).toBeUndefined()
    expect((await ctx.retrieval.stats()).workspaces).toBe(1)
    await ctx.retrieval.close()
    expect((await ctx.retrieval.stats()).workspaces).toBe(0)

    const reopened = await mount(root)
    const hits = await reopened.retrieval.search({ workspaceRoot: root, query: 'needle', mode: 'lexical', limit: 10 })
    expect(hits.items.map(hit => hit.id).sort()).toEqual(['a', 'b', 'c'])
    await reopened.retrieval.close()
  })

  it('filters by scope, path, session, type, surface, and time', async () => {
    const root = await workspace()
    const ctx = await mount(root)
    await ctx.retrieval.indexDocuments({
      workspaceRoot: root,
      documents: [
        { id: 'w', scope: 'workspace', text: 'target text', path: 'src/w.ts' },
        { id: 's', scope: 'session', text: 'target text', sessionId: 's1', seq: 1, time: 10, type: 'user/message', surface: 'current' },
      ],
    })
    const workspaceHits = await ctx.retrieval.search({
      workspaceRoot: root, query: 'target', mode: 'lexical', limit: 10, filter: { scope: ['workspace'], path: ['src/w.ts'] },
    })
    expect(workspaceHits.items.map(hit => hit.id)).toEqual(['w'])
    const sessionHits = await ctx.retrieval.search({
      workspaceRoot: root, query: 'target', mode: 'lexical', limit: 10,
      filter: { scope: ['session'], sessionId: ['s1'], type: ['user/message'], surface: ['current'], timeFrom: 5, timeTo: 15 },
    })
    expect(sessionHits.items.map(hit => hit.id)).toEqual(['s'])
    await ctx.retrieval.close()
  })

  it('removes documents by id and by metadata filter', async () => {
    const root = await workspace()
    const ctx = await mount(root)
    await ctx.retrieval.indexDocuments({
      workspaceRoot: root,
      documents: [
        { id: 'a', scope: 'workspace', text: 'needle one', path: 'a.ts' },
        { id: 'b', scope: 'workspace', text: 'needle two', path: 'b.ts' },
        { id: 'c', scope: 'session', text: 'needle three', sessionId: 's' },
      ],
    })
    await ctx.retrieval.removeDocuments({ workspaceRoot: root, ids: ['a'] })
    await ctx.retrieval.removeDocuments({ workspaceRoot: root, sessionId: 's' })
    expect((await ctx.retrieval.search({ workspaceRoot: root, query: 'needle', mode: 'lexical', limit: 10 })).items.map(hit => hit.id)).toEqual(['b'])
    await ctx.retrieval.close()
  })

  it('searches semantically and hybrid with caller-supplied vectors', async () => {
    const root = await workspace()
    const ctx = await mount(root, { vectorDimension: 3 })
    await ctx.retrieval.indexDocuments({
      workspaceRoot: root,
      documents: [
        { id: 'x', scope: 'workspace', text: 'apple fruit', path: 'a.ts', vector: [1, 0, 0] },
        { id: 'y', scope: 'workspace', text: 'banana fruit', path: 'b.ts', vector: [0, 1, 0] },
      ],
    })
    expect((await ctx.retrieval.search({ workspaceRoot: root, query: 'ignored', mode: 'semantic', limit: 10, vector: [1, 0, 0] })).items[0]?.id).toBe('x')
    expect((await ctx.retrieval.search({ workspaceRoot: root, query: 'banana', mode: 'hybrid', limit: 10, vector: [0, 1, 0] })).items[0]?.id).toBe('y')
    await ctx.retrieval.close()
  })

  it('rejects unsupported vectors, invalid dimensions, and missing semantic configuration', async () => {
    const root = await workspace()
    const lexical = await mount(root)
    await expect(lexical.retrieval.indexDocuments({
      workspaceRoot: root,
      documents: [{ id: 'a', scope: 'workspace', text: 'x', vector: [1] }],
    })).rejects.toMatchObject({ code: 'RETRIEVAL_SEMANTIC_DISABLED' })
    await expect(lexical.retrieval.search({ workspaceRoot: root, query: 'x', mode: 'semantic', limit: 1, vector: [1] }))
      .rejects.toMatchObject({ code: 'RETRIEVAL_SEMANTIC_DISABLED' })
    await lexical.retrieval.close()

    const semantic = await mount(root, { vectorDimension: 3 })
    await expect(semantic.retrieval.indexDocuments({
      workspaceRoot: root,
      documents: [{ id: 'a', scope: 'workspace', text: 'x', vector: [1, 2] }],
    })).rejects.toMatchObject({ code: 'RETRIEVAL_INVALID_REQUEST' })
    await expect(semantic.retrieval.search({ workspaceRoot: root, query: 'x', mode: 'semantic', limit: 1, vector: [1, 2] }))
      .rejects.toMatchObject({ code: 'RETRIEVAL_INVALID_REQUEST' })
    await semantic.retrieval.close()
  })

  it('rejects invalid requests and cursors', async () => {
    const root = await workspace()
    const ctx = await mount(root)
    await expect(ctx.retrieval.search({ workspaceRoot: root, query: '   ', mode: 'lexical', limit: 1 }))
      .rejects.toBeInstanceOf(RetrievalError)
    await expect(ctx.retrieval.search({ workspaceRoot: root, query: 'x', mode: 'lexical', limit: 0 }))
      .rejects.toMatchObject({ code: 'RETRIEVAL_INVALID_REQUEST' })
    await expect(ctx.retrieval.search({ workspaceRoot: root, query: 'x', mode: 'lexical', limit: 1, cursor: 'bad' }))
      .rejects.toMatchObject({ code: 'RETRIEVAL_INVALID_REQUEST' })
    await expect(ctx.retrieval.removeDocuments({ workspaceRoot: root }))
      .rejects.toMatchObject({ code: 'RETRIEVAL_INVALID_REQUEST' })
    await expect(ctx.retrieval.search({ workspaceRoot: join(root, 'missing'), query: 'x', mode: 'lexical', limit: 1 }))
      .rejects.toMatchObject({ code: 'RETRIEVAL_WORKSPACE_NOT_FOUND' })
    await ctx.retrieval.close()
  })
})
