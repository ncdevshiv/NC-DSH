import { afterEach, describe, expect, it } from 'vitest'
import { mkdtemp, rm } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { provideWorkingDirectoryFixture } from '@deepseek-ai/dsh-agent-loop-testkit'
import { Context } from '@deepseek-ai/cordis'
import { ToolCallId } from '@deepseek-ai/dsh-llm'
import SystemPrompt from '@deepseek-ai/dsh-system-prompt'
import ToolRuntime from '@deepseek-ai/dsh-tools'
import ZvecRetrievalService from '@deepseek-ai/dsh-retrieval-zvec'
import * as ToolRetrieval from '@deepseek-ai/dsh-tool-retrieval'
import { DEFAULT_MAX_RESULTS, DEFAULT_TIMEOUT_MS } from '@deepseek-ai/dsh-tool-retrieval'

const directories: string[] = []
const contexts: Context[] = []
const signal = new AbortController().signal
let counter = 0

afterEach(async () => {
  for (const ctx of contexts.splice(0)) await ctx.fiber.dispose()
  for (const directory of directories.splice(0)) await rm(directory, { recursive: true, force: true, maxRetries: 10, retryDelay: 200 })
})

async function workspace(): Promise<string> {
  const directory = await mkdtemp(join(tmpdir(), 'dsh-tool-retrieval-'))
  directories.push(directory)
  return directory
}

function text(result: { content: { type: string; text?: string }[] }): string {
  return result.content.filter(block => block.type === 'text').map(block => block.text).join('')
}

async function mount(root: string, config: Record<string, unknown> = {}, vectorDimension?: number): Promise<Context> {
  const ctx = new Context()
  contexts.push(ctx)
  provideWorkingDirectoryFixture(ctx, root)
  await ctx.plugin(SystemPrompt)
  await ctx.plugin(ToolRuntime)
  await ctx.plugin(ZvecRetrievalService, {
    embedding: false,
    dataDir: '.dsh',
    ...vectorDimension === undefined ? {} : { vectorDimension },
  })
  await ctx.plugin(ToolRetrieval, config)
  return ctx
}

function agent(ctx: Context, root: string) {
  return { session: { header: { id: 'session-retrieval', cwd: root } }, ctx } as never
}

async function call(ctx: Context, root: string, name: string, args: unknown) {
  return ctx.tools.execute({
    signal,
    callId: ToolCallId(`retrieval-${String(++counter)}`),
    name,
    arguments: args,
    agent: agent(ctx, root),
  })
}

describe('workspace retrieval tools', () => {
  it('searches and indexes documents through the real tool pipeline', async () => {
    const root = await workspace()
    const ctx = await mount(root)
    const indexed = await call(ctx, root, 'workspace_index', {
      documents: [
        { id: 'file-a', text: 'alpha beta needle', path: 'src/a.ts' },
        { id: 'file-b', text: 'beta gamma needle', path: 'src/b.ts' },
      ],
    })
    expect(indexed.isError).toBe(false)
    expect(text(indexed)).toBe('Indexed 2 workspace documents.')

    const found = await call(ctx, root, 'workspace_search', { query: 'needle' })
    expect(found.isError).toBe(false)
    expect(text(found)).toContain('Workspace matches (2):')
    expect(text(found)).toContain('src/a.ts')
    expect(text(found)).toContain('src/b.ts')

    const filtered = await call(ctx, root, 'workspace_search', { query: 'needle', path: ['src/a.ts'] })
    expect(text(filtered)).toContain('Workspace matches (1):')
    expect(text(filtered)).not.toContain('src/b.ts')
    await ctx.retrieval.close()
  })

  it('returns session hits with session metadata and honors filters and cursors', async () => {
    const root = await workspace()
    const ctx = await mount(root, { maxResults: 2 })
    await call(ctx, root, 'workspace_index', {
      documents: [
        { id: 'e1', text: 'needle session one', session_id: 's1', seq: 1, type: 'user/message', surface: 'current', time: '2026-01-01T00:00:00Z' },
        { id: 'e2', text: 'needle session two', session_id: 's2', seq: 2, type: 'assistant/message', surface: 'shadowed', time: '2026-02-01T00:00:00Z' },
        { id: 'e3', text: 'needle session three', session_id: 's1', seq: 3, type: 'tool/result', surface: 'log-only', time: '2026-03-01T00:00:00Z' },
      ],
    })
    const first = await call(ctx, root, 'workspace_search', { query: 'needle' })
    expect(text(first)).toContain('Workspace matches (2):')
    expect(text(first)).toMatch(/session s\d seq \d/)
    expect(text(first)).toContain('More matches exist.')
    const second = await call(ctx, root, 'workspace_search', {
      query: 'needle',
      session_id: ['s2'],
      event_types: ['assistant/message'],
      event_surfaces: ['shadowed'],
      time_from: '2026-01-15T00:00:00Z',
      time_to: '2026-02-15T00:00:00Z',
    })
    expect(text(second)).toContain('session s2 seq 2')
    expect(text(second)).not.toContain('s1')
    await ctx.retrieval.close()
  })

  it('requires a semantic query vector in a vector deployment', async () => {
    const root = await workspace()
    const ctx = await mount(root, {}, 3)
    const indexed = await call(ctx, root, 'workspace_index', {
      documents: [
        { id: 'x', text: 'apple fruit', path: 'a.ts', time: '2026-01-01T00:00:00Z', vector: [1, 0, 0] },
        { id: 'y', text: 'banana fruit', path: 'b.ts', time: '2026-06-01T00:00:00Z', vector: [0, 1, 0] },
      ],
    })
    expect(indexed.isError).toBe(false)
    const semantic = await call(ctx, root, 'workspace_search', { query: 'fruit', mode: 'semantic' })
    expect(semantic.isError).toBe(true)
    expect(semantic.error).toMatchObject({ info: { code: 'RETRIEVAL_TOOL_FAILED' } })
    await ctx.retrieval.close()
  })

  it('reports no matches and rejects invalid arguments', async () => {
    const root = await workspace()
    const ctx = await mount(root)
    expect(text(await call(ctx, root, 'workspace_search', { query: 'nothing' }))).toBe('No workspace matches found.')

    const invalidTime = await call(ctx, root, 'workspace_search', { query: 'x', time_from: 'not-a-time' })
    expect(invalidTime.isError).toBe(true)
    expect(invalidTime.error).toMatchObject({ info: { code: 'RETRIEVAL_TOOL_INVALID_ARGUMENTS' } })

    const reversed = await call(ctx, root, 'workspace_search', {
      query: 'x',
      time_from: '2026-02-01T00:00:00Z',
      time_to: '2026-01-01T00:00:00Z',
    })
    expect(reversed.isError).toBe(true)
    expect(text(reversed)).toContain('time_from must not exceed time_to')

    const emptyIndex = await call(ctx, root, 'workspace_index', { documents: [] })
    expect(emptyIndex.isError).toBe(true)
    expect(text(emptyIndex)).toContain('workspace_index requires at least one document')

    const noAgent = await ctx.tools.execute({
      signal, callId: ToolCallId(`retrieval-${String(++counter)}`), name: 'workspace_search', arguments: { query: 'x' },
    })
    expect(noAgent.isError).toBe(true)
    expect(noAgent.error).toMatchObject({ info: { code: 'RETRIEVAL_TOOL_MISSING_AGENT' } })
    await ctx.retrieval.close()
  })

  it('falls back to the working directory when the header has none and translates provider failures', async () => {
    const root = await workspace()
    const ctx = new Context()
    contexts.push(ctx)
    provideWorkingDirectoryFixture(ctx, root)
    await ctx.plugin(SystemPrompt)
    await ctx.plugin(ToolRuntime)
    const warnings: string[] = []
    ctx.logger.warn = ((message: unknown) => { warnings.push(String(message)) }) as typeof ctx.logger.warn
    ctx.provide('retrieval', {
      search: (): Promise<never> => Promise.reject(new Error('provider offline')),
      indexDocuments: (): Promise<never> => Promise.reject(new Error('raw failure')),
    } as never)
    await ctx.plugin(ToolRetrieval, {})

    const headerless = { session: { header: { id: 'headerless' } }, ctx } as never
    const searched = await ctx.tools.execute({
      signal, callId: ToolCallId('fallback-search'), name: 'workspace_search', arguments: { query: 'x' }, agent: headerless,
    })
    expect(searched.isError).toBe(true)
    expect(searched.error).toMatchObject({ info: { code: 'RETRIEVAL_TOOL_FAILED' } })
    const indexed = await ctx.tools.execute({
      signal, callId: ToolCallId('fallback-index'), name: 'workspace_index', arguments: { documents: [{ id: 'a', text: 'x' }] }, agent: headerless,
    })
    expect(indexed.isError).toBe(true)
    expect(warnings.length).toBeGreaterThanOrEqual(2)
    expect(text(searched)).toContain('workspace search is unavailable')
    expect(text(indexed)).toContain('workspace indexing is unavailable')
  })

  it('rejects invalid tool configuration at load', async () => {
    const ctx = new Context()
    contexts.push(ctx)
    provideWorkingDirectoryFixture(ctx, process.cwd())
    ctx.provide('retrieval', { search: () => Promise.reject(new Error('unused')) } as never)
    await ctx.plugin(SystemPrompt)
    await ctx.plugin(ToolRuntime)
    await expect(ctx.plugin(ToolRetrieval, { maxResults: 0 })).rejects.toThrow('maxResults')
    await expect(ctx.plugin(ToolRetrieval, { timeoutMs: 0 })).rejects.toThrow('timeoutMs')
  })

  it('rejects absent configuration defaults instead of accepting zero bounds', () => {
    expect(DEFAULT_MAX_RESULTS).toBeGreaterThan(0)
    expect(DEFAULT_TIMEOUT_MS).toBeGreaterThan(0)
  })

  it('truncates long snippets and presents the search call', async () => {
    const root = await workspace()
    const ctx = await mount(root)
    await call(ctx, root, 'workspace_index', { documents: [{ id: 'long', text: `${'prefix '.repeat(60)}needle`, path: 'long.ts' }] })
    const found = await call(ctx, root, 'workspace_search', { query: 'needle' })
    expect(text(found)).toContain('…')
    const definition = ctx.tools.get('workspace_search')
    if (definition === undefined) throw new Error('workspace_search must be registered')
    const view = definition.presentCall?.({ query: 'needle' })
    expect(view).toMatchObject({ card: 'generic', kind: 'search', title: 'Search workspace', rawInput: 'needle' })
    await ctx.retrieval.close()
  })
})
