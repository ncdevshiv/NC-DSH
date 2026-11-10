import { afterEach, describe, expect, it } from 'vitest'
import { mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { Context } from '@deepseek-ai/cordis'
import ZvecRetrievalService from '@deepseek-ai/dsh-retrieval-zvec'

const directories: string[] = []
const contexts: Context[] = []

afterEach(async () => {
  for (const ctx of contexts.splice(0)) await ctx.fiber.dispose()
  for (const directory of directories.splice(0)) await rm(directory, { recursive: true, force: true, maxRetries: 10, retryDelay: 200 })
})

async function workspace(): Promise<string> {
  const directory = await mkdtemp(join(tmpdir(), 'dsh-retrieval-negative-'))
  directories.push(directory)
  return directory
}

async function mount(config: Record<string, unknown>): Promise<Context> {
  const ctx = new Context()
  contexts.push(ctx)
  await ctx.plugin(ZvecRetrievalService, { embedding: false, dataDir: '.dsh', ...config })
  return ctx
}

describe('zvec retrieval provider negative paths', () => {
  it('treats empty index and remove calls as no-ops', async () => {
    const root = await workspace()
    const ctx = await mount({})
    await ctx.retrieval.indexDocuments({ workspaceRoot: root, documents: [] })
    await ctx.retrieval.removeDocuments({ workspaceRoot: root, ids: [] })
    expect((await ctx.retrieval.stats()).workspaces).toBe(0)
  })

  it('rejects a workspace root that is empty, missing, or a file', async () => {
    const root = await workspace()
    const ctx = await mount({})
    await expect(ctx.retrieval.indexDocuments({
      workspaceRoot: '',
      documents: [{ id: 'a', scope: 'workspace', text: 'x' }],
    })).rejects.toMatchObject({ code: 'RETRIEVAL_WORKSPACE_NOT_FOUND' })
    const file = join(root, 'file.txt')
    await writeFile(file, 'not a directory')
    await expect(ctx.retrieval.indexDocuments({
      workspaceRoot: file,
      documents: [{ id: 'a', scope: 'workspace', text: 'x' }],
    })).rejects.toMatchObject({ code: 'RETRIEVAL_WORKSPACE_NOT_FOUND' })
  })

  it('reports an unopenable collection and keeps later work failing loud', async () => {
    const root = await workspace()
    const ctx = await mount({})
    const collectionPath = join(root, '.dsh', 'retrieval')
    await mkdir(collectionPath, { recursive: true })
    await writeFile(join(collectionPath, 'manifest.json'), 'not a collection')
    await expect(ctx.retrieval.indexDocuments({
      workspaceRoot: root,
      documents: [{ id: 'a', scope: 'workspace', text: 'x' }],
    })).rejects.toMatchObject({ code: 'RETRIEVAL_PROVIDER_FAILED' })
  })

  it('rejects a closed provider and invalid cursor payloads', async () => {
    const root = await workspace()
    const ctx = await mount({})
    await ctx.retrieval.indexDocuments({
      workspaceRoot: root,
      documents: [{ id: 'a', scope: 'workspace', text: 'needle text' }],
    })
    await ctx.retrieval.close()
    await expect(ctx.retrieval.search({ workspaceRoot: root, query: 'needle', mode: 'lexical', limit: 1 }))
      .rejects.toMatchObject({ code: 'RETRIEVAL_PROVIDER_FAILED' })
    const reopened = await mount({})
    await expect(reopened.retrieval.search({
      workspaceRoot: root,
      query: 'needle',
      mode: 'lexical',
      limit: 1,
      cursor: Buffer.from(JSON.stringify({ version: 2, offset: 0 }), 'utf8').toString('base64url'),
    })).rejects.toMatchObject({ code: 'RETRIEVAL_INVALID_REQUEST' })
    await expect(reopened.retrieval.search({
      workspaceRoot: root,
      query: 'needle',
      mode: 'lexical',
      limit: 1,
      cursor: Buffer.from(JSON.stringify({ version: 1, offset: -1 }), 'utf8').toString('base64url'),
    })).rejects.toMatchObject({ code: 'RETRIEVAL_INVALID_REQUEST' })
  })

  it('rejects a reversed filter range and unsupported remove filters', async () => {
    const root = await workspace()
    const ctx = await mount({})
    await ctx.retrieval.indexDocuments({
      workspaceRoot: root,
      documents: [{ id: 'a', scope: 'workspace', text: 'needle text' }],
    })
    await expect(ctx.retrieval.search({
      workspaceRoot: root,
      query: 'needle',
      mode: 'lexical',
      limit: 1,
      filter: { timeFrom: 10, timeTo: 5 },
    })).rejects.toMatchObject({ code: 'RETRIEVAL_INVALID_FILTER' })
    await expect(ctx.retrieval.search({
      workspaceRoot: root,
      query: 'needle',
      mode: 'lexical',
      limit: 1,
      filter: { timeFrom: Number.NaN },
    })).rejects.toMatchObject({ code: 'RETRIEVAL_INVALID_FILTER' })
  })

  it('removes documents by path and session filters', async () => {
    const root = await workspace()
    const ctx = await mount({})
    await ctx.retrieval.indexDocuments({
      workspaceRoot: root,
      documents: [
        { id: 'a', scope: 'workspace', text: 'needle one', path: 'src/a.ts' },
        { id: 'b', scope: 'session', text: 'needle two', sessionId: 's' },
      ],
    })
    await ctx.retrieval.removeDocuments({ workspaceRoot: root, path: 'src/a.ts' })
    await ctx.retrieval.removeDocuments({ workspaceRoot: root, sessionId: 's' })
    expect((await ctx.retrieval.search({ workspaceRoot: root, query: 'needle', mode: 'lexical', limit: 10 })).items).toEqual([])
  })

  it('searches hybrid with a filter and returns a session hit with every metadata field', async () => {
    const root = await workspace()
    const ctx = await mount({ maxLimit: 10, vectorDimension: 3 })
    await ctx.retrieval.indexDocuments({
      workspaceRoot: root,
      documents: [
        { id: 's', scope: 'session', text: 'needle metadata', sessionId: 'sess', seq: 4, type: 'user/message', surface: 'current', time: 7, vector: [1, 0, 0] },
        { id: 'w', scope: 'workspace', text: 'needle file', path: 'src/a.ts', vector: [0, 1, 0] },
      ],
    })
    const hybrid = await ctx.retrieval.search({
      workspaceRoot: root, query: 'needle', mode: 'hybrid', limit: 10, vector: [1, 0, 0], filter: { scope: ['session'] },
    })
    expect(hybrid.items.map(hit => hit.id)).toEqual(['s'])
    const hit = (await ctx.retrieval.search({ workspaceRoot: root, query: 'needle', mode: 'lexical', limit: 10 }))
      .items.find(item => item.id === 's')
    expect(hit).toEqual(expect.objectContaining({
      id: 's', scope: 'session', text: 'needle metadata', sessionId: 'sess', seq: 4, type: 'user/message', surface: 'current', time: 7,
    }))
    expect(typeof hit?.score).toBe('number')
  })

  it('rejects a vectorDimension that is not a positive integer', async () => {
    const ctx = new Context()
    contexts.push(ctx)
    await expect(ctx.plugin(ZvecRetrievalService, { embedding: false, vectorDimension: 0 }))
      .rejects.toThrow('vectorDimension')
  })

  it('filters scope, surface, and path clauses', async () => {
    const root = await workspace()
    const ctx = await mount({ maxLimit: 10 })
    await ctx.retrieval.indexDocuments({
      workspaceRoot: root,
      documents: [
        { id: 'a', scope: 'workspace', text: 'needle', path: 'src/a.ts' },
        { id: 'b', scope: 'session', text: 'needle', sessionId: 's', surface: 'current' },
      ],
    })
    const scoped = await ctx.retrieval.search({
      workspaceRoot: root, query: 'needle', mode: 'lexical', limit: 10, filter: { scope: ['session'], surface: ['current'] },
    })
    expect(scoped.items.map(hit => hit.id)).toEqual(['b'])
    const pathFiltered = await ctx.retrieval.search({
      workspaceRoot: root, query: 'needle', mode: 'lexical', limit: 10, filter: { path: ['src/a.ts'] },
    })
    expect(pathFiltered.items.map(hit => hit.id)).toEqual(['a'])
  })
})
