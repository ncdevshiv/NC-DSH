/**
 * The durable store of change summaries: a summary recorded by one Host is
 * served by a later Host over the same storage root, a record that fails its
 * schema is moved aside instead of failing the boot, and a store this Host
 * cannot use leaves the live summaries in place.
 */
import { mkdir, readFile, readdir, writeFile } from 'node:fs/promises'
import { join } from 'node:path'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { Context } from '@deepseek-ai/cordis'
import SessionStore, { SessionId } from '@deepseek-ai/dsh-session'
import LocalSubprocessRuntime from '@deepseek-ai/dsh-subprocess-local'
import LocalFileSystem from '@deepseek-ai/dsh-fs-local'
import * as WorkspaceChanges from '../src/index.ts'
import { WorkspaceChangesStore, workspaceChangesDomain } from '../src/storage.ts'
import type { WorkspaceChangesSummary } from '../src/types.ts'
import { endTurn, git, scratchDir, settle, startTurn, storageStack, toolCall } from './support.ts'

const cleanups: Array<() => Promise<unknown>> = []
afterEach(async () => {
  for (const cleanup of cleanups.reverse()) await cleanup()
  cleanups.length = 0
})

/** A repository with one tracked file. */
async function repository(): Promise<string> {
  const cwd = await scratchDir('dsh-wc-store-repo-', cleanups)
  git(cwd, 'init', '-q', '-b', 'main')
  await writeFile(join(cwd, 'a.txt'), 'one\n')
  git(cwd, 'add', '-A'); git(cwd, 'commit', '-q', '-m', 'init')
  return cwd
}

/** A Host with the storage stack on `root` and the plugin mounted. */
async function host(root: string, config: Partial<WorkspaceChanges.Config> = {}): Promise<Context> {
  const ctx = new Context()
  cleanups.push(() => ctx.fiber.dispose())
  await storageStack(ctx, root)
  await ctx.plugin(SessionStore)
  await ctx.plugin(LocalSubprocessRuntime)
  await ctx.plugin(LocalFileSystem)
  await ctx.plugin(WorkspaceChanges, config as WorkspaceChanges.Config)
  return ctx
}

/** One turn's summary, as the recorder builds it. */
function summary(files: WorkspaceChangesSummary['files']): WorkspaceChangesSummary {
  return { turn: 1, cwd: '/workspace', files, total: files.length, added: 0, deleted: 0 }
}

describe('the durable change-summary store', () => {
  it('serves a summary recorded by an earlier Host', { timeout: 30_000 }, async () => {
    const root = await scratchDir('dsh-wc-store-', cleanups)
    const cwd = await repository()
    const first = await host(root)
    const session = first.sessions.create(SessionId('stored'), { meta: { cwd } })
    startTurn(session, 1)
    await settle(first, session)
    await writeFile(join(cwd, 'a.txt'), 'one\ntwo\n')
    toolCall(session, 1, 'bash', { command: 'x' })
    endTurn(session, 1)
    await settle(first, session)
    const announcement = session.snapshotEvents().findLast(event => event.type === 'workspace/changes')
    expect(announcement).toBeDefined()
    const seq = announcement!.seq
    expect(first.workspaceChanges.summary(session.id, seq)).toMatchObject({ turn: 1, total: 1 })

    // The stored document holds the file list the announcement names.
    const document = JSON.parse(await readFile(join(root, workspaceChangesDomain.name, 'sessions', `${session.id}.json`), 'utf8')) as {
      record: { summaries: Record<string, { total: number }> }
    }
    expect(document.record.summaries[String(seq)]).toMatchObject({ total: 1 })

    // A later Host over the same root serves the stored summary.
    await first.fiber.dispose()
    const second = await host(root)
    expect(second.workspaceChanges.summary(session.id, seq)).toMatchObject({
      turn: 1, total: 1, files: [{ path: 'a.txt', display: 'a.txt', added: 1, deleted: 0 }],
    })
    expect(second.workspaceChanges.summary(session.id, seq + 1)).toBeUndefined()
  })

  it('moves a record that fails its schema aside instead of failing the boot', { timeout: 30_000 }, async () => {
    const root = await scratchDir('dsh-wc-store-', cleanups)
    const directory = join(root, workspaceChangesDomain.name, 'sessions')
    await mkdir(directory, { recursive: true })
    await writeFile(join(directory, 'broken.json'), JSON.stringify({ version: 1, record: { summaries: { '7': { turn: 0 } } } }))
    const ctx = await host(root)
    expect(ctx.workspaceChanges.summary(SessionId('broken'), 7)).toBeUndefined()
    const entries = await readdir(directory)
    expect(entries.filter(entry => entry.startsWith('broken.json.bak.'))).toHaveLength(1)
  })

  it('keeps the listed file’s own flags across the store', { timeout: 30_000 }, async () => {
    const root = await scratchDir('dsh-wc-store-', cleanups)
    const ctx = new Context()
    cleanups.push(() => ctx.fiber.dispose())
    await storageStack(ctx, root)
    const store = await WorkspaceChangesStore.open(ctx.storageDomain, () => {})
    const files = [
      { path: 'a.txt', display: 'a.txt', added: 1, deleted: 0 },
      { path: 'bin.dat', display: 'bin.dat', added: 0, deleted: 0, binary: true as const },
      { path: 'big.txt', display: 'big.txt', added: 0, deleted: 0, oversized: true as const },
    ]
    await store.persist(SessionId('flags'), 3, summary(files))
    expect(store.summary(SessionId('flags'), 3)).toEqual(summary(files))
    expect(store.summary(SessionId('flags'), 4)).toBeUndefined()
    await store.close()
  })

  it('serves the live summaries when the store cannot open', { timeout: 30_000 }, async () => {
    const root = await scratchDir('dsh-wc-store-', cleanups)
    await writeFile(join(root, workspaceChangesDomain.name), 'not a directory')
    const ctx = new Context()
    cleanups.push(() => ctx.fiber.dispose())
    const warnings: string[] = []
    vi.spyOn(ctx.logger, 'warn').mockImplementation((message: unknown) => { warnings.push(String(message)) })
    await storageStack(ctx, root)
    await ctx.plugin(SessionStore)
    await ctx.plugin(LocalSubprocessRuntime)
    await ctx.plugin(LocalFileSystem)
    await ctx.plugin(WorkspaceChanges)
    expect(warnings.join('\n')).toContain('durable change-summary store is unavailable')
    expect(ctx.workspaceChanges).toBeDefined()
  })

  it('warns when a summary cannot be stored and keeps serving it', { timeout: 30_000 }, async () => {
    const root = await scratchDir('dsh-wc-store-', cleanups)
    const cwd = await repository()
    const ctx = new Context()
    cleanups.push(() => ctx.fiber.dispose())
    const warnings: string[] = []
    vi.spyOn(ctx.logger, 'warn').mockImplementation((message: unknown) => { warnings.push(String(message)) })
    await storageStack(ctx, root)
    await ctx.plugin(SessionStore)
    await ctx.plugin(LocalSubprocessRuntime)
    await ctx.plugin(LocalFileSystem)
    await ctx.plugin(WorkspaceChanges)
    // A Session id the store cannot name as a document key.
    const session = ctx.sessions.create(SessionId('unusable/id'), { meta: { cwd } })
    startTurn(session, 1)
    await settle(ctx, session)
    await writeFile(join(cwd, 'a.txt'), 'one\ntwo\n')
    toolCall(session, 1, 'bash', { command: 'x' })
    endTurn(session, 1)
    await settle(ctx, session)
    const announcement = session.snapshotEvents().findLast(event => event.type === 'workspace/changes')
    expect(announcement).toBeDefined()
    expect(warnings.join('\n')).toContain('was not stored')
    expect(ctx.workspaceChanges.summary(session.id, announcement!.seq)).toMatchObject({ turn: 1, total: 1 })
  })
})
