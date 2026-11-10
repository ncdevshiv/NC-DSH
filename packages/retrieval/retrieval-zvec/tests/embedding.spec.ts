import { afterEach, describe, expect, it, vi } from 'vitest'
import { mkdtemp, rm } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { Context } from '@deepseek-ai/cordis'
import ZvecRetrievalService from '@deepseek-ai/dsh-retrieval-zvec'

const initialized = vi.fn()
const embedDocuments = vi.fn(async (texts: readonly string[]) => texts.map(() => Array.from({ length: 384 }, () => 1 / Math.sqrt(384))))
const embedQuery = vi.fn(async () => Array.from({ length: 384 }, () => 1 / Math.sqrt(384)))

vi.mock('fastembed', () => ({
  ExecutionProvider: { CPU: 'cpu' },
  EmbeddingModel: { BGESmallENV15: 'fast-bge-small-en-v1.5', CUSTOM: 'custom' },
  FlagEmbedding: {
    init: async (options: { cacheDir: string }) => {
      initialized(options.cacheDir)
      return {
        passageEmbed: async function* (texts: readonly string[]) {
          yield await embedDocuments(texts)
        },
        queryEmbed: embedQuery,
      }
    },
  },
}))

const directories: string[] = []
const contexts: Context[] = []

afterEach(async () => {
  for (const ctx of contexts.splice(0)) await ctx.fiber.dispose()
  initialized.mockClear()
  embedDocuments.mockClear()
  embedQuery.mockClear()
  for (const directory of directories.splice(0)) await rm(directory, { recursive: true, force: true, maxRetries: 10, retryDelay: 200 })
})

async function workspace(): Promise<string> {
  const directory = await mkdtemp(join(tmpdir(), 'dsh-retrieval-embedding-'))
  directories.push(directory)
  return directory
}

describe('zvec local embeddings', () => {
  it('embeds documents and queries with a configured local model', async () => {
    const root = await workspace()
    const ctx = new Context()
    contexts.push(ctx)
    await ctx.plugin(ZvecRetrievalService, {
      dataDir: '.dsh',
      embedding: { model: 'fast-bge-small-en-v1.5', maxLength: 512, batchSize: 2 },
    })
    expect((ctx.retrieval as unknown as { config: { vectorDimension?: number } }).config.vectorDimension).toBe(384)
    await ctx.retrieval.indexDocuments({
      workspaceRoot: root,
      documents: [
        { id: 'a', scope: 'workspace', text: 'alpha text' },
        { id: 'b', scope: 'workspace', text: 'beta text' },
      ],
    })
    expect(embedDocuments).toHaveBeenCalledOnce()
    const semantic = await ctx.retrieval.search({ workspaceRoot: root, query: 'alpha', mode: 'semantic', limit: 2 })
    expect(semantic.items.map(hit => hit.id).sort()).toEqual(['a', 'b'])
    expect(embedQuery).toHaveBeenCalledWith('alpha')
    expect(initialized).toHaveBeenCalledWith(join(root, '.dsh', 'retrieval-models'))
    const hybrid = await ctx.retrieval.search({ workspaceRoot: root, query: 'alpha', mode: 'hybrid', limit: 2 })
    expect(hybrid.items.length).toBeGreaterThan(0)
    await ctx.retrieval.close()
  })

  it('encodes every document through the embedder while preserving supplied vectors', async () => {
    const root = await workspace()
    const ctx = new Context()
    contexts.push(ctx)
    await ctx.plugin(ZvecRetrievalService, { dataDir: '.dsh', embedding: { model: 'fast-bge-small-en-v1.5' } })
    await ctx.retrieval.indexDocuments({
      workspaceRoot: root,
      documents: [
        { id: 'supplied', scope: 'workspace', text: 'first', vector: Array.from({ length: 384 }, () => 0.5) },
        { id: 'embedded', scope: 'workspace', text: 'second' },
      ],
    })
    expect(embedDocuments).toHaveBeenCalledOnce()
    const semantic = await ctx.retrieval.search({ workspaceRoot: root, query: 'first', mode: 'semantic', limit: 2 })
    expect(semantic.items.map(hit => hit.id).sort()).toEqual(['embedded', 'supplied'])
    await ctx.retrieval.close()
  })

  it('rejects an embedding model whose dimension does not match an explicit vectorDimension', async () => {
    const ctx = new Context()
    await expect(ctx.plugin(ZvecRetrievalService, {
      dataDir: '.dsh',
      vectorDimension: 3,
      embedding: { model: 'fast-bge-small-en-v1.5' },
    })).rejects.toThrow('does not match embedding model dimension')
  })
})
