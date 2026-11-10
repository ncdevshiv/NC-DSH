/** The `ncag` Remote projection: snapshot shaping, clamping, and error translation. */

import { Context } from '@deepseek-ai/cordis'
import { describe, expect, it, vi } from 'vitest'
import { RemoteError } from '@deepseek-ai/dsh-typert-protocol'
import { NcagController } from '../src/index.ts'

/** A project record the orchestrator would publish. */
const PROJECT = {
  id: 'p1', name: 'demo', description: 'a demo project', repositoryPath: '/repo', baseBranch: 'main', status: 'ACTIVE',
}

/** One task record. */
const TASK = {
  id: 't1', projectId: 'p1', title: 'build it', description: '', requiredCapability: 'implementation',
  status: 'READY', priority: 'P1', attempts: 0, reviewCycles: 0, statusReason: null,
  assignedAgentId: null, deadlineMs: null,
}

/** The orchestrator surface the controller reads. */
function orchestratorStub() {
  return {
    snapshot: vi.fn(async () => ({
      generatedAt: '2026-11-09T00:00:00.000Z',
      projects: [{
        project: PROJECT, requests: 2, tasks: [TASK], agents: [], decisionsPending: 1, leases: 0, costUsd: 0.25,
      }],
      usage: { inputTokens: 10, outputTokens: 20, costUsd: 0.25, calls: 3 },
      events: [{ sequence: 1, at: 'now', kind: 'task/transition', projectId: 'p1', taskId: 't1', agentId: null, summary: 'moved', data: {} }],
      audit: { records: 4, brokenAt: null },
      routes: [{ provider: 'p', model: 'm', tier: 'standard', inputPerMillion: 1, outputPerMillion: 2 }],
      profiles: 2,
    })),
    project: vi.fn((id: string) => {
      if (id !== 'p1') throw new Error(`ncag: project ${id} does not exist`)
      return PROJECT
    }),
    trace: vi.fn(() => ({
      projectId: 'p1',
      correlationId: 'c1',
      audit: [{ sequence: 1, at: 'now', category: 'task', action: 'x', actor: 'a', projectId: 'p1', taskId: null, agentId: null }],
      events: [],
      messages: [],
      toolCalls: [{ sequence: 1, at: 'now', agentId: 'a1', taskId: null, tool: 'fs', ok: true, error: null, output: '', durationMs: 3 }],
      tasks: [],
      agents: [],
    })),
    store: { recentEvents: vi.fn(() => []) },
    agents: { summarize: vi.fn(() => []), listLive: vi.fn(() => []) },
    decisions: { pending: vi.fn(() => []), history: vi.fn(() => []) },
  }
}

/** Mount the controller over a stubbed platform service. */
async function bench() {
  const ctx = new Context()
  const orchestrator = orchestratorStub()
  ctx.provide('ncag', { orchestrator, createProjectFromWorkspace: vi.fn(() => PROJECT) } as never)
  ctx.provide('typert', {} as never)
  const controller = new NcagController(ctx, { maxEvents: 500 })
  return { ctx, controller, orchestrator }
}

describe('NcagController', () => {
  it('projects the snapshot without the event payload and reports live counters', async () => {
    const { controller, orchestrator } = await bench()
    orchestrator.agents.listLive.mockReturnValue([{ id: 'a1' }, { id: 'a2' }] as never)
    orchestrator.decisions.pending.mockReturnValue([{ id: 'd1' }] as never)

    const snapshot = await controller.snapshot({})

    expect(snapshot.projects).toHaveLength(1)
    expect(snapshot.projects[0]).toMatchObject({ requests: 2, decisionsPending: 1, costUsd: 0.25 })
    expect(snapshot.events[0]).toEqual({
      sequence: 1, at: 'now', kind: 'task/transition', projectId: 'p1', taskId: 't1', agentId: null, summary: 'moved',
    })
    expect(snapshot.routes[0]).toEqual({ provider: 'p', model: 'm', tier: 'standard', inputPerMillion: 1, outputPerMillion: 2 })
    expect(snapshot.liveAgents).toBe(2)
    expect(snapshot.pendingDecisions).toBe(1)
  })

  it('filters the snapshot to one project', async () => {
    const { controller } = await bench()
    expect((await controller.snapshot({ projectId: 'other' })).projects).toHaveLength(0)
  })

  it('clamps the requested event window to the deployment ceiling', async () => {
    const { controller, orchestrator } = await bench()
    await controller.snapshot({ eventLimit: 9_000 })
    expect(orchestrator.store.recentEvents).toHaveBeenCalledWith(500, undefined)
    controller.recentEvents({ limit: 0 })
    expect(orchestrator.store.recentEvents).toHaveBeenLastCalledWith(1, undefined)
  })

  it('translates a missing record into ncag/not-found', async () => {
    const { controller } = await bench()
    expect(() => controller.trace({ projectId: 'gone', correlationId: 'c1' }))
      .toThrow(expect.objectContaining({ code: 'ncag/not-found' }))
    expect(() => controller.pauseProject({ projectId: 'gone' }))
      .toThrow(RemoteError)
  })

  it('translates any other orchestrator refusal into ncag/invalid', async () => {
    const { controller, orchestrator } = await bench()
    orchestrator.trace.mockImplementation(() => { throw new Error('planner rejected the request') })
    try {
      controller.trace({ projectId: 'p1', correlationId: 'c1' })
      expect.unreachable()
    } catch (error) {
      expect(error).toMatchObject({ code: 'ncag/invalid', details: { operation: 'trace' } })
    }
  })

  it('anchors a project without a repository path at the first workspace', await_(async () => {
    const { controller } = await bench()
    expect(controller.createProject({ name: 'n', description: 'd' })).toEqual(PROJECT)
  }))
})

/** Wrap a zero-argument async body as a synchronous test callback. */
function await_(run: () => Promise<void>): () => Promise<void> {
  return run
}
