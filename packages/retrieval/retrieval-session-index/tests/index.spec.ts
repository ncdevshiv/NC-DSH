import { afterEach, describe, expect, it } from 'vitest'
import { mkdtemp, rm } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { Context } from '@deepseek-ai/cordis'
import { createUserMessage } from '@deepseek-ai/dsh-llm'
import { provideWorkingDirectoryFixture } from '@deepseek-ai/dsh-agent-loop-testkit'
import SessionStore, { SessionId } from '@deepseek-ai/dsh-session'
import SessionProjectionRegistry from '@deepseek-ai/dsh-session-projection'
import ZvecRetrievalService from '@deepseek-ai/dsh-retrieval-zvec'
import * as SessionIndex from '@deepseek-ai/dsh-retrieval-session-index'

const directories: string[] = []

afterEach(async () => {
  for (const directory of directories.splice(0)) await rm(directory, { recursive: true, force: true, maxRetries: 10, retryDelay: 200 })
})

async function workspace(): Promise<string> {
  const directory = await mkdtemp(join(tmpdir(), 'dsh-retrieval-session-index-'))
  directories.push(directory)
  return directory
}

async function mount(root: string, config: { maxTextBytes?: number } = {}): Promise<Context> {
  const ctx = new Context()
  provideWorkingDirectoryFixture(ctx, root)
  await ctx.plugin(SessionProjectionRegistry)
  await ctx.plugin(SessionStore)
  await ctx.plugin(ZvecRetrievalService, { embedding: false, dataDir: '.dsh' })
  await ctx.plugin(SessionIndex, config)
  return ctx
}

function userEvent(text: string) {
  return createUserMessage({
    content: [{ type: 'text', text }],
    source: { kind: 'user' },
  })
}

describe('session event indexing', () => {
  it('indexes committed semantic events and keeps the workspace collection', async () => {
    const root = await workspace()
    const ctx = await mount(root)
    const session = ctx.sessions.create(SessionId('session-indexed'), {
      meta: { createdAt: 1, cwd: root, isSeeded: false },
    })
    session.append('user/message', userEvent('the workspace indexable needle'), { surfaceOp: 'append' })
    await ctx.retrieval.flush()
    const page = await ctx.retrieval.search({ workspaceRoot: root, query: 'needle', mode: 'lexical', limit: 10 })
    expect(page.items.map(hit => hit.id)).toEqual(['session:session-indexed:0'])
    expect(page.items[0]).toMatchObject({ scope: 'session', sessionId: 'session-indexed', seq: 0 })
    await ctx.retrieval.close()
  })

  it('skips sessions without a project directory and events without semantic text', async () => {
    const root = await workspace()
    const ctx = await mount(root)
    const bare = ctx.sessions.create(SessionId('bare-session'), { meta: { createdAt: 1, isSeeded: false } })
    bare.append('user/message', userEvent('unowned text'), { surfaceOp: 'append' })
    const owned = ctx.sessions.create(SessionId('owned-session'), {
      meta: { createdAt: 1, cwd: root, isSeeded: false },
    })
    owned.append('turn/start', {} as never)
    await ctx.retrieval.flush()
    expect((await ctx.retrieval.search({ workspaceRoot: root, query: 'unowned', mode: 'lexical', limit: 10 })).items).toEqual([])
    expect((await ctx.retrieval.stats()).documents).toBe(0)
    await ctx.retrieval.close()
  })

  it('truncates event text to the configured byte budget', async () => {
    const root = await workspace()
    const ctx = await mount(root, { maxTextBytes: 12 })
    const session = ctx.sessions.create(SessionId('truncated'), {
      meta: { createdAt: 1, cwd: root, isSeeded: false },
    })
    session.append('user/message', userEvent('prefix-limited-needle'), { surfaceOp: 'append' })
    await ctx.retrieval.flush()
    const page = await ctx.retrieval.search({ workspaceRoot: root, query: 'prefix', mode: 'lexical', limit: 10 })
    expect(page.items).toHaveLength(1)
    expect(page.items[0]?.text).toBe('prefix-limit')
    expect((await ctx.retrieval.search({ workspaceRoot: root, query: 'needle', mode: 'lexical', limit: 10 })).items).toEqual([])
    await ctx.retrieval.close()
  })

  it('rejects a non-positive text budget at load', async () => {
    const root = await workspace()
    const ctx = new Context()
    provideWorkingDirectoryFixture(ctx, root)
    await ctx.plugin(SessionProjectionRegistry)
    await ctx.plugin(SessionStore)
    await ctx.plugin(ZvecRetrievalService, { embedding: false, dataDir: '.dsh' })
    await expect(ctx.plugin(SessionIndex, { maxTextBytes: 0 }))
      .rejects.toThrow('maxTextBytes')
  })

  it('logs a failed indexing write without rejecting the committed append', async () => {
    const root = await workspace()
    const ctx = new Context()
    provideWorkingDirectoryFixture(ctx, root)
    await ctx.plugin(SessionProjectionRegistry)
    await ctx.plugin(SessionStore)
    const warnings: string[] = []
    ctx.logger.warn = ((message: unknown) => { warnings.push(String(message)) }) as typeof ctx.logger.warn
    ctx.provide('retrieval', {
      indexDocuments: (): Promise<void> => Promise.reject(new Error('provider offline')),
    } as never)
    await ctx.plugin(SessionIndex, {})
    const session = ctx.sessions.create(SessionId('failing'), {
      meta: { createdAt: 1, cwd: root, isSeeded: false },
    })
    session.append('user/message', userEvent('write fails here'), { surfaceOp: 'append' })
    await ctx.fiber.dispose()
    expect(warnings.some(line => line.includes('provider offline'))).toBe(true)
  })
})
