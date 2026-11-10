import { mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterEach, describe, expect, it } from 'vitest'
import { Context, Service } from '@deepseek-ai/cordis'
import SystemPrompt from '@deepseek-ai/dsh-system-prompt'
import ToolRuntime from '@deepseek-ai/dsh-tools'
import NcagService from '../src/index.ts'
import * as ToolNcag from '../../tool-ncag/src/index.ts'
import { ToolCallId } from '@deepseek-ai/dsh-llm'
import { newId } from '@deepseek-ai/dsh-ncag-core'
import { createNcagTools } from '../../tool-ncag/src/tools.ts'

/** A workspace registry stand-in so the service can anchor a project without a real workspace plugin. */
class FakeWorkspaceRegistry extends Service {
  private readonly path: string

  constructor(ctx: Context, path: string) {
    super(ctx, 'workspaceRegistry')
    this.path = path
  }

  list(): readonly { readonly path: string }[] {
    return [{ path: this.path }]
  }
}

const contexts: Context[] = []
const directories: string[] = []

afterEach(async () => {
  while (contexts.length > 0) {
    const ctx = contexts.pop()
    if (ctx !== undefined) await ctx.fiber.dispose()
  }
  while (directories.length > 0) {
    const directory = directories.pop()
    if (directory !== undefined) rmSync(directory, { recursive: true, force: true })
  }
})

async function setup(withWorkspace: boolean, databasePath?: string): Promise<Context> {
  const root = mkdtempSync(join(tmpdir(), 'ncag-service-'))
  directories.push(root)
  const ctx = new Context()
  contexts.push(ctx)
  await ctx.plugin(SystemPrompt)
  await ctx.plugin(ToolRuntime)
  if (withWorkspace) new FakeWorkspaceRegistry(ctx, join(root, 'workspace'))
  await ctx.plugin(NcagService, {
    databasePath: databasePath ?? join(root, 'platform.db'),
    workspaceRoot: join(root, 'workspaces'),
    modelPricing: JSON.stringify([{ provider: 'p', model: 'm', inputPerMillion: 1, outputPerMillion: 2 }]),
    preferredProviders: [],
    leaseTtlMs: 900_000,
    maxConcurrentAgents: 2,
    maxStepsPerTask: 4,
    maxOutputTokens: 1024,
    healthIntervalMs: 60_000,
    stalledAfterMs: 600_000,
    repeatedToolThreshold: 4,
  })
  await ctx.plugin(ToolNcag)
  return ctx
}

let callNumber = 0
/** One tool execution result the service spec reads back. */
interface ToolResult {
  readonly content: { readonly type: string; readonly text?: string }[]
  readonly value: unknown
  readonly isError: boolean
}

async function run(ctx: Context, name: string, args: unknown): Promise<ToolResult> {
  return ctx.tools.execute({
    signal: new AbortController().signal,
    callId: ToolCallId(`ncag-call-${++callNumber}`),
    name,
    arguments: args,
  }) as Promise<ToolResult>
}

function text(result: { content: { type: string; text?: string }[] }): string {
  return result.content.filter(block => block.type === 'text').map(block => block.text).join('')
}

describe('ncag harness service and model-facing tools', () => {
  it('mounts the platform service and registers every tool', async () => {
    const ctx = await setup(true)
    expect(ctx.ncag).toBeInstanceOf(NcagService)
    expect(ctx.tools.schemas().map(schema => schema.name).sort())
      .toEqual(['ncag_decide', 'ncag_events', 'ncag_project', 'ncag_request', 'ncag_status'])
  })

  it('shares one platform across mounts that name the same database', async () => {
    const root = mkdtempSync(join(tmpdir(), 'ncag-service-shared-'))
    directories.push(root)
    const database = join(root, 'platform.db')
    const first = await setup(false, database)
    const firstOrchestrator = first.ncag.orchestrator
    const second = await setup(false, database)
    expect(second.ncag.orchestrator).toBe(firstOrchestrator)

    // Releasing one mount leaves the scheduler serving the other.
    await first.fiber.dispose()
    const created = await run(second, 'ncag_project', {
      name: 'shared', description: 'd', repositoryPath: join(root, 'repo'),
    })
    expect(created.isError).toBe(false)
    const projectId = (created.value as { id: string }).id
    expect(second.ncag.orchestrator.project(projectId as never).name).toBe('shared')

    // Releasing the last mount stops it; the next mount restores durable state.
    await second.fiber.dispose()
    const third = await setup(false, database)
    expect(third.ncag.orchestrator).not.toBe(firstOrchestrator)
    expect(third.ncag.orchestrator.project(projectId as never).name).toBe('shared')
  })

  it('keeps an in-memory platform private to its own mount', async () => {
    // Two mounts cannot share an in-memory database; each owns its store.
    const first = await setup(false, ':memory:')
    const second = await setup(false, ':memory:')
    expect(second.ncag.orchestrator).not.toBe(first.ncag.orchestrator)
    expect((await run(second, 'ncag_status', {})).isError).toBe(false)
  })

  it('creates a project from an explicit path and from the first registered workspace', async () => {
    const ctx = await setup(true)
    const explicit = await run(ctx, 'ncag_project', { name: 'explicit', description: 'd', repositoryPath: join(tmpdir(), 'ncag-service-explicit') })
    expect(explicit.isError).toBe(false)
    expect(text(explicit)).toContain('explicit')
    const anchored = await run(ctx, 'ncag_project', { name: 'anchored', description: 'd' })
    expect(anchored.isError).toBe(false)
    expect((anchored.value as { repositoryPath: string }).repositoryPath).toContain('workspace')
  })

  it('reads one project timeline and refuses an unknown project', async () => {
    const ctx = await setup(false)
    const created = await run(ctx, 'ncag_project', { name: 'timeline', description: 'd', repositoryPath: join(tmpdir(), 'ncag-service-timeline') })
    const projectId = (created.value as { id: string }).id
    await run(ctx, 'ncag_request', { projectId, text: 'Implement the timeline feature.' })
    const events = await run(ctx, 'ncag_events', { projectId, limit: '3' })
    expect(events.isError).toBe(false)
    const listed = (events.value as { events: { kind: string }[] }).events
    expect(listed.length).toBe(3)
    expect(listed.every(event => typeof event.kind === 'string')).toBe(true)
    // Every recorded timeline event must belong to the requested project.
    const unknown = await run(ctx, 'ncag_events', { projectId: 'prj_missing' })
    expect(unknown.isError).toBe(true)
    expect(text(unknown)).toContain('does not exist')
  })

  it('refuses to anchor a project when the harness has no workspace', async () => {
    const ctx = await setup(false)
    const result = await run(ctx, 'ncag_project', { name: 'homeless', description: 'd' })
    expect(result.isError).toBe(true)
    expect(text(result)).toContain('no registered workspace')
  })

  it('renders every result defensively and reports a broken audit chain', async () => {
    const ctx = await setup(true)
    const created = await run(ctx, 'ncag_project', { name: 'instructed', description: 'd', repositoryPath: join(tmpdir(), 'ncag-service-instructed'), instructions: 'be careful' })
    const projectId = (created.value as { id: string }).id
    ctx.ncag.orchestrator.agents.createInstance({
      profile: ctx.ncag.orchestrator.agents.select('general-implementation'),
      projectId: projectId as never, taskId: null, model: { provider: 'p', model: 'm' },
    })
    // Tamper the chain so the status tool reports the broken state it observed.
    ctx.ncag.orchestrator.store.recordAudit({ category: 'task', action: 'x', actor: 'o', correlationId: newId('correlation') })
    ctx.ncag.orchestrator.store.db.prepare('UPDATE audit SET action = ? WHERE sequence = 1').run('tampered')
    const status = await run(ctx, 'ncag_status', { projectId })
    expect(status.isError).toBe(false)
    expect(text(status)).toContain('broken at')
    expect(text(status)).toContain('agent ')

    const decision = ctx.ncag.orchestrator.decisions.propose({
      projectId: projectId as never, taskId: null, category: 'technical-recommendation', author: 'orchestrator',
      problem: 'p', debate: null, recommendation: 'rec',
    })
    expect((await run(ctx, 'ncag_decide', { decisionId: decision.id, outcome: 'rec' })).isError).toBe(false)

    // Rendering a partial value exercises the defensive display fallbacks.
    const definitions = createNcagTools(ctx)
    for (const definition of definitions) definition.output.render({}, {})
    const requestTool = definitions.find(definition => definition.name === 'ncag_request')
    requestTool?.output.render({}, { tasks: [{}] })
    await ctx.fiber.dispose()
    await ctx.fiber.dispose()
  })

  it('reports a missing NC-AG service from every tool', async () => {
    const ctx = new Context()
    contexts.push(ctx)
    await ctx.plugin(SystemPrompt)
    await ctx.plugin(ToolRuntime)
    for (const definition of createNcagTools(ctx)) ctx.tools.register(definition)
    const result = await ctx.tools.execute({
      signal: new AbortController().signal,
      callId: ToolCallId('ncag-bare-call'),
      name: 'ncag_status',
      arguments: {},
    })
    expect((result as { isError: boolean }).isError).toBe(true)
  })

  it('submits a request, reports status and records a decision answer', async () => {
    const ctx = await setup(false)
    const created = await run(ctx, 'ncag_project', { name: 'work', description: 'd', repositoryPath: join(tmpdir(), 'ncag-service-work') })
    const projectId = (created.value as { id: string }).id

    const submitted = await run(ctx, 'ncag_request', { projectId, text: 'Implement a small feature.' })
    expect(submitted.isError).toBe(false)
    expect(text(submitted)).toContain('Implement the requested change')

    const all = await run(ctx, 'ncag_status', {})
    expect(all.isError).toBe(false)
    expect(text(all)).toContain('audit chain')
    const one = await run(ctx, 'ncag_status', { projectId })
    expect(text(one)).toContain('work')

    const decision = ctx.ncag.orchestrator.decisions.propose({
      projectId: projectId as never, taskId: null, category: 'user-reserved', author: 'orchestrator',
      problem: 'spend more?', debate: null, recommendation: 'yes',
    })
    const answered = await run(ctx, 'ncag_decide', { decisionId: decision.id, outcome: 'yes', rationale: 'approved' })
    expect(answered.isError).toBe(false)
    expect(text(answered)).toContain(decision.id)
  })

  it('mounts with no price table and no deployment environment', async () => {
    const root = mkdtempSync(join(tmpdir(), 'ncag-service-free-'))
    directories.push(root)
    const ctx = new Context()
    contexts.push(ctx)
    await ctx.plugin(SystemPrompt)
    await ctx.plugin(ToolRuntime)
    await ctx.plugin(NcagService, {
      databasePath: join(root, 'platform.db'),
      workspaceRoot: join(root, 'workspaces'),
      modelPricing: '[]',
      preferredProviders: [],
      leaseTtlMs: 900_000,
      maxConcurrentAgents: 2,
      maxStepsPerTask: 4,
      maxOutputTokens: 1024,
      healthIntervalMs: 60_000,
      stalledAfterMs: 600_000,
      repeatedToolThreshold: 4,
    })
    expect(ctx.ncag.orchestrator).toBeDefined()
    const project = ctx.ncag.orchestrator.createProject({
      name: 'free', description: 'no price table', repositoryPath: join(root, 'repo'),
    })
    expect(project.name).toBe('free')
  })
})
