/** Session-fork boundaries, lineage, and inherited model routing. */

import { describe, expect, it, vi } from 'vitest'
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import path from 'node:path'
import { Context } from '@deepseek-ai/cordis'
import AgentRegistry, { agentEvents } from '@deepseek-ai/dsh-agent'
import type { Agent, AgentHandle, CreateAgentOptions } from '@deepseek-ai/dsh-agent'
import { createUserMessage, ReasoningEffortId } from '@deepseek-ai/dsh-llm'
import type { LlmCallConfig } from '@deepseek-ai/dsh-llm'
import SessionStore from '@deepseek-ai/dsh-session'
import type { Session, SessionEvent, SessionHeader, SessionId } from '@deepseek-ai/dsh-session'
import SystemPrompt from '@deepseek-ai/dsh-system-prompt'
import { apply as applyTurnRestore } from '@deepseek-ai/dsh-turn-restore'
import UserQuestionService from '@deepseek-ai/dsh-user-questions'
import type { Workspace, WorkspaceId } from '@deepseek-ai/dsh-workspace'
import type { RpcRequest } from '@deepseek-ai/dsh-host-apiproxy/api/rpc'
import { RpcId } from '@deepseek-ai/dsh-host-apiproxy/api/rpc'
import { createApiProxy } from '@deepseek-ai/dsh-host-apiproxy'

const sid = (id: string): SessionId => id as SessionId

let nextRpc = 1
function request<P>(payload: P): RpcRequest<P> {
  return { rpcId: RpcId(`fork-${String(nextRpc++)}`), payload }
}

async function composed(
  workspaces: readonly Workspace[] = [],
  registry?: Record<string, unknown>,
): Promise<Context> {
  const ctx = new Context()
  await ctx.plugin(SessionStore)
  await ctx.plugin(SystemPrompt, { persona: '' })
  await ctx.plugin(AgentRegistry)
  await ctx.plugin(UserQuestionService)
  ctx.provide('workspaceRegistry', registry ?? { list: () => workspaces } as never)
  ctx.agents.setFactory({
    createAgent: async (ownerCtx: Context, options: CreateAgentOptions): Promise<AgentHandle> => {
      const session = ctx.sessions.create(options.sessionId, {
        ...options.seed === undefined ? {} : { seed: [...options.seed] },
        ...options.meta === undefined ? {} : { meta: options.meta },
      })
      const agent = {} as Agent
      const agentCtx = ownerCtx.extend({ agent })
      Object.assign(agent, { id: session.id, session, status: 'idle', ctx: agentCtx })
      await options.setup?.(agentCtx)
      ctx.agents.register(agent)
      return { agent, dispose: () => Promise.resolve() }
    },
    resume: () => Promise.reject(new Error('fork test sources are live')),
  })
  return ctx
}

/** Tail turn appended after the completed ones: left open, or closed as aborted (a stopped turn). */
type Tail = 'none' | 'open' | 'aborted'

function liveAgent(
  ctx: Context,
  id: string,
  turns: number,
  tail: Tail = 'none',
  lineage: { parentSession?: SessionId; origin?: 'subagent' } = {},
): Session {
  const session = ctx.sessions.create(sid(id), { meta: { cwd: '/proj', ...lineage } })
  for (let turn = 1; turn <= turns; turn++) {
    session.append('turn/start', { turn })
    session.append('user/message', createUserMessage({
      content: [{ type: 'text', text: `prompt ${String(turn)}` }],
      source: { kind: 'user' },
    }), { surfaceOp: 'append' })
    session.append('turn/end', { turn, reason: { kind: 'completed' } })
  }
  if (tail !== 'none') {
    session.append('turn/start', { turn: turns + 1 })
    session.append('user/message', createUserMessage({
      content: [{ type: 'text', text: 'open prompt' }],
      source: { kind: 'user' },
    }), { surfaceOp: 'append' })
    if (tail === 'aborted') session.append('turn/end', {
      turn: turns + 1,
      reason: { kind: 'aborted', reason: { kind: 'user' } },
    })
  }
  ctx.agents.register({ id: session.id, session, status: 'idle', ctx } as Agent)
  return session
}

const api = (ctx: Context) => createApiProxy(ctx, {
  defaultModelSelection: () => ({ provider: 'default-provider', model: 'default-model' }),
  cwd: '/tmp',
})

describe('sessions.fork', () => {
  it('cuts at the anchored completed turn and records lineage and cwd', async () => {
    const ctx = await composed()
    const source = liveAgent(ctx, 'session-source', 2)
    const response = await api(ctx).sessions.fork(request({ sessionId: source.id, atSeq: 1 }))
    expect(response.result.ok).toBe(true)
    if (!response.result.ok) return
    const child = ctx.sessions.get(response.result.value.sessionId)
    expect(child?.events.map(event => event.type)).toEqual([
      'turn/start', 'user/message', 'turn/end', 'session/end-seed',
    ])
    expect(child?.header.parentSession).toBe(source.id)
    expect(child?.header.cwd).toBe('/proj')
    await ctx.fiber.dispose()
  })

  it('attaches a subagent fork to its nearest workspace-owning ancestor', async () => {
    const accounted: SessionId[] = []
    const attachSession = vi.fn<(sessionId: SessionId) => Promise<void>>()
      .mockResolvedValue(undefined)
    const workspace = {
      sessionIds: accounted,
      attachSession,
    } as unknown as Workspace
    const ctx = await composed([workspace])
    const owner = liveAgent(ctx, 'session-owner', 1)
    accounted.push(owner.id)
    const child = liveAgent(ctx, 'session-child', 1, 'none', {
      parentSession: owner.id,
      origin: 'subagent',
    })
    const grandchild = liveAgent(ctx, 'session-grandchild', 1, 'none', {
      parentSession: child.id,
      origin: 'subagent',
    })
    ctx.provide('sessionQuery', {
      traceSession: vi.fn(() => Promise.resolve({
        target: { header: grandchild.header, live: true, persisted: false },
        ancestors: [
          { header: child.header, live: true, persisted: false },
          { header: owner.header, live: true, persisted: false },
        ],
        descendants: [],
        complete: true,
        root: { header: owner.header, live: true, persisted: false },
      })),
    } as never)

    const response = await api(ctx).sessions.fork(request({ sessionId: grandchild.id }))

    expect(response.result.ok).toBe(true)
    if (!response.result.ok) return
    expect(attachSession).toHaveBeenCalledWith(response.result.value.sessionId)
    expect(ctx.sessions.get(response.result.value.sessionId)?.header).toMatchObject({
      parentSession: grandchild.id,
      cwd: '/proj',
    })
    expect(ctx.sessions.get(response.result.value.sessionId)?.header.origin).toBeUndefined()
    await ctx.fiber.dispose()
  })

  it('forks a persisted subagent without resuming its Agent', async () => {
    const ctx = await composed()
    const sourceId = sid('session-cold-subagent')
    const parentId = sid('session-cold-parent')
    const header: SessionHeader = {
      version: 0,
      id: sourceId,
      createdAt: 1,
      cwd: '/proj',
      parentSession: parentId,
      origin: 'subagent',
    }
    const events = [
      { type: 'turn/start', seq: 0, time: 1, data: { turn: 1, trigger: { kind: 'message', source: { kind: 'user' } } } },
      {
        type: 'user/message',
        seq: 1,
        time: 2,
        data: createUserMessage({ content: [{ type: 'text', text: 'work' }], source: { kind: 'user' } }),
        surfaceOp: 'append',
      },
      { type: 'turn/end', seq: 2, time: 3, data: { turn: 1, reason: { kind: 'completed' } } },
    ] as SessionEvent[]
    ctx.provide('sessionPersistence', {
      list: () => Promise.resolve([header]),
      inspect: () => Promise.resolve({ meta: header, events }),
    } as never)
    ctx.provide('sessionQuery', {
      traceSession: () => Promise.resolve({
        target: { header, live: false, persisted: true },
        ancestors: [],
        descendants: [],
        complete: true,
        root: { header, live: false, persisted: true },
      }),
    } as never)
    const resume = vi.spyOn(ctx.agents, 'resume')

    const response = await api(ctx).sessions.fork(request({ sessionId: sourceId }))

    expect(response.result.ok).toBe(true)
    if (!response.result.ok) return
    expect(resume).not.toHaveBeenCalled()
    expect(ctx.agents.get(sourceId)).toBeUndefined()
    expect(ctx.sessions.get(response.result.value.sessionId)?.header).toMatchObject({
      parentSession: sourceId,
      cwd: '/proj',
    })
    expect(ctx.sessions.get(response.result.value.sessionId)?.header.origin).toBeUndefined()
    await ctx.fiber.dispose()
  })

  it('uses the last completed turn only for omitted and past-end anchors', async () => {
    const ctx = await composed()
    const source = liveAgent(ctx, 'session-tail', 2, 'open')
    const proxy = api(ctx)
    const expectedTypes = [
      'turn/start', 'user/message', 'turn/end',
      'turn/start', 'user/message', 'turn/end',
      'session/end-seed',
    ]
    const omitted = await proxy.sessions.fork(request({ sessionId: source.id }))
    expect(omitted.result.ok).toBe(true)
    if (omitted.result.ok) {
      expect(ctx.sessions.get(omitted.result.value.sessionId)?.events.map(event => event.type))
        .toEqual(expectedTypes)
    }
    const pastEnd = await proxy.sessions.fork(request({ sessionId: source.id, atSeq: 999 }))
    expect(pastEnd.result.ok).toBe(true)
    if (pastEnd.result.ok) {
      expect(ctx.sessions.get(pastEnd.result.value.sessionId)?.events.map(event => event.type))
        .toEqual(expectedTypes)
    }
    await ctx.fiber.dispose()
  })

  it('cuts through an aborted turn: stopped is closed, not open', async () => {
    const ctx = await composed()
    const source = liveAgent(ctx, 'session-aborted', 1, 'aborted')
    // What a stopped message's fork button anchors on: the frozen node sits
    // one event before its turn/end, floored client-side to that event's seq.
    const anchor = (source.events.at(-1)?.seq ?? 0) - 1
    const response = await api(ctx).sessions.fork(request({ sessionId: source.id, atSeq: anchor }))
    expect(response.result.ok).toBe(true)
    if (!response.result.ok) return
    expect(ctx.sessions.get(response.result.value.sessionId)?.events.map(event => event.type)).toEqual([
      'turn/start', 'user/message', 'turn/end',
      'turn/start', 'user/message', 'turn/end',
      'session/end-seed',
    ])
    await ctx.fiber.dispose()
  })

  it('rejects an in-log anchor whose turn is still open', async () => {
    const ctx = await composed()
    const source = liveAgent(ctx, 'session-open', 1, 'open')
    const anchor = source.events.at(-1)?.seq ?? 0
    const response = await api(ctx).sessions.fork(request({ sessionId: source.id, atSeq: anchor }))
    expect(response.result).toMatchObject({
      ok: false,
      error: { code: 'fork-unavailable', details: { sessionId: source.id } },
    })
    if (!response.result.ok) expect(response.result.error.message).toMatch(/has not completed/)
    await ctx.fiber.dispose()
  })

  it('installs the latest logged model selection before the child can run', async () => {
    const ctx = await composed()
    const source = liveAgent(ctx, 'session-routed', 1)
    source.append('request/header', {
      header: {
        config: {
          provider: 'inherited-provider',
          model: 'inherited-model',
          reasoningEffort: ReasoningEffortId('high'),
        },
      },
      reason: 'initial',
    })
    const response = await api(ctx).sessions.fork(request({ sessionId: source.id }))
    expect(response.result.ok).toBe(true)
    if (!response.result.ok) return
    const child = ctx.agents.get(response.result.value.sessionId)
    if (child === undefined) throw new Error('fork did not publish the child agent')
    const assembly = await child.ctx.systemPrompt.assemble()
    expect(assembly.variables).toMatchObject({
      provider: 'inherited-provider',
      model: 'inherited-model',
    })
    const fallback: LlmCallConfig = { provider: 'default-provider', model: 'default-model' }
    await expect(agentEvents(child.ctx, child).waterfall(
      'agent/request', { turn: 1, step: 0, signal: new AbortController().signal }, () => Promise.resolve(fallback),
    )).resolves.toMatchObject({
      provider: 'inherited-provider',
      model: 'inherited-model',
      reasoningEffort: 'high',
    })
    await ctx.fiber.dispose()
  })

  it('an explicit workspaceId retargets the child cwd and attachment', async () => {
    const attachSession = vi.fn<(sessionId: SessionId) => Promise<void>>()
      .mockResolvedValue(undefined)
    const workspace = {
      id: 'ws-target',
      path: '/target',
      sessionIds: [],
      attachSession,
    } as unknown as Workspace
    const ctx = await composed([workspace], {
      list: () => [workspace],
      get: (id: string) => id === 'ws-target' ? workspace : undefined,
    })
    // A chat-mode source at the Host cwd, accounted nowhere.
    const source = liveAgent(ctx, 'session-chat', 1)

    const response = await api(ctx).sessions.fork(
      request({ sessionId: source.id, workspaceId: 'ws-target' as WorkspaceId }),
    )

    expect(response.result.ok).toBe(true)
    if (!response.result.ok) return
    expect(attachSession).toHaveBeenCalledWith(response.result.value.sessionId)
    const child = ctx.sessions.get(response.result.value.sessionId)
    expect(child?.header.cwd).toBe('/target')
    expect(child?.header.parentSession).toBe(source.id)
    // Retention, not a move: the seeded history still carries the source turns.
    expect(child?.events.some(event => event.type === 'user/message')).toBe(true)
    await ctx.fiber.dispose()
  })

  it('an unknown explicit workspace fails with workspace-not-found', async () => {
    const ctx = await composed([], { list: () => [], get: () => undefined })
    const source = liveAgent(ctx, 'session-nows', 1)

    const response = await api(ctx).sessions.fork(
      request({ sessionId: source.id, workspaceId: 'ws-missing' as WorkspaceId }),
    )

    expect(response.result).toMatchObject({
      ok: false,
      error: { code: 'workspace-not-found', details: { workspaceId: 'ws-missing' } },
    })
    await ctx.fiber.dispose()
  })

  it('a failed retarget attachment publishes the child and reports it', async () => {
    const attachSession = vi.fn<(sessionId: SessionId) => Promise<void>>()
      .mockRejectedValue(new Error('ledger unavailable'))
    const workspace = {
      id: 'ws-late',
      path: '/target',
      sessionIds: [],
      attachSession,
    } as unknown as Workspace
    const ctx = await composed([workspace], {
      list: () => [workspace],
      get: (id: string) => id === 'ws-late' ? workspace : undefined,
    })
    const source = liveAgent(ctx, 'session-attachfail', 1)

    const response = await api(ctx).sessions.fork(
      request({ sessionId: source.id, workspaceId: 'ws-late' as WorkspaceId }),
    )

    expect(response.result).toMatchObject({
      ok: false,
      error: { code: 'workspace-attach-failed' },
    })
    if (response.result.ok) return
    const details = response.result.error.details as { sessionId: SessionId }
    // The child exists with the target cwd even though the account slot failed.
    expect(ctx.sessions.get(details.sessionId)?.header.cwd).toBe('/target')
    await ctx.fiber.dispose()
  })

  it('beforeSeq drops the whole turn containing the anchor', async () => {
    const ctx = await composed()
    const source = liveAgent(ctx, 'session-edit', 2)
    // Turn 2's user/message is seq 4; the cut must not include turn 2 at all.
    const response = await api(ctx).sessions.fork(request({ sessionId: source.id, beforeSeq: 4 }))
    expect(response.result.ok).toBe(true)
    if (!response.result.ok) return
    const child = ctx.sessions.get(response.result.value.sessionId)
    expect(child?.events.map(event => event.type)).toEqual([
      'turn/start', 'user/message', 'turn/end', 'session/end-seed',
    ])
    expect(child?.header.parentSession).toBe(source.id)
    expect(child?.header.cwd).toBe('/proj')
    await ctx.fiber.dispose()
  })

  it('beforeSeq on the first turn forks an empty-seed child', async () => {
    const ctx = await composed()
    const source = liveAgent(ctx, 'session-firstedit', 1)
    // Turn 1's user/message is seq 1; nothing precedes it to retain.
    const response = await api(ctx).sessions.fork(request({ sessionId: source.id, beforeSeq: 1 }))
    expect(response.result.ok).toBe(true)
    if (!response.result.ok) return
    const child = ctx.sessions.get(response.result.value.sessionId)
    expect(child?.events.map(event => event.type)).toEqual(['session/end-seed'])
    expect(child?.header.parentSession).toBe(source.id)
    await ctx.fiber.dispose()
  })

  it('beforeSeq refuses an anchor inside an open turn', async () => {
    const ctx = await composed()
    const source = liveAgent(ctx, 'session-editopen', 1, 'open')
    const anchor = source.events.at(-1)?.seq ?? 0
    const response = await api(ctx).sessions.fork(request({ sessionId: source.id, beforeSeq: anchor }))
    expect(response.result).toMatchObject({
      ok: false,
      error: { code: 'fork-unavailable', details: { sessionId: source.id } },
    })
    if (!response.result.ok) expect(response.result.error.message).toMatch(/has not completed/)
    await ctx.fiber.dispose()
  })

  it('beforeSeq refuses an anchor outside every turn', async () => {
    const ctx = await composed()
    const source = liveAgent(ctx, 'session-editgap', 2)
    // A trailing standalone title sits after the last turn/end: a rewind
    // anchor there addresses no turn.
    source.append('session/title', {
      title: 'after turn 2',
      messageSeqs: [],
      source: { kind: 'user' },
    })
    const anchor = source.events.at(-1)?.seq ?? 0
    const response = await api(ctx).sessions.fork(request({ sessionId: source.id, beforeSeq: anchor }))
    expect(response.result).toMatchObject({
      ok: false,
      error: { code: 'fork-unavailable', details: { sessionId: source.id } },
    })
    if (!response.result.ok) expect(response.result.error.message).toMatch(/outside every turn/)
    await ctx.fiber.dispose()
  })

  it('a beforeSeq rewind restores the discarded turns write bases', async () => {
    const ctx = await composed()
    applyTurnRestore(ctx)
    const workspace = await mkdtemp(path.join(tmpdir(), 'dsh-fork-restore-'))
    const session = ctx.sessions.create(sid('session-restore'), { meta: { cwd: workspace } })
    session.append('turn/start', { turn: 1 })
    session.append('user/message', createUserMessage({
      content: [{ type: 'text', text: 'one' }],
      source: { kind: 'user' },
    }), { surfaceOp: 'append' })
    session.append('turn/end', { turn: 1, reason: { kind: 'completed' } })
    session.append('turn/start', { turn: 2 })
    session.append('user/message', createUserMessage({
      content: [{ type: 'text', text: 'two' }],
      source: { kind: 'user' },
    }), { surfaceOp: 'append' })
    session.append('tool/call', { turn: 2, step: 1, callId: 'c1' as never, name: 'write', arguments: '{}' })
    session.append('tool/result', {
      turn: 2,
      step: 1,
      message: {
        id: 'm1' as never,
        role: 'user',
        content: [{
          type: 'tool-result', toolCallId: 'c1' as never, isError: false,
          content: [{ type: 'text', text: 'ok' }],
        }],
        source: { kind: 'tool', callId: 'c1' as never },
      },
      meta: { diffs: [], basis: { path: 'a.md', op: 'update', before: 'old', after: 'new' } },
    }, { surfaceOp: 'append' })
    session.append('turn/end', { turn: 2, reason: { kind: 'completed' } })
    await writeFile(path.join(workspace, 'a.md'), 'new', 'utf8')
    ctx.agents.register({ id: session.id, session, status: 'idle', ctx } as Agent)

    const response = await api(ctx).sessions.fork(request({ sessionId: session.id, beforeSeq: 4 }))

    expect(response.result.ok).toBe(true)
    if (!response.result.ok) return
    expect(response.result.value.restoreReport).toMatchObject({ restored: 1, conflicts: [] })
    expect(await readFile(path.join(workspace, 'a.md'), 'utf8')).toBe('old')
    await rm(workspace, { recursive: true, force: true })
    await ctx.fiber.dispose()
  })

  it('skips the workspace restore while the source agent is running', async () => {
    const ctx = await composed()
    applyTurnRestore(ctx)
    const source = liveAgent(ctx, 'session-restorebusy', 1)
    // The registry entry is the same liveAgent handle: flip its status in place
    // (the property is readonly by contract; runtime assignment is the test's
    // only way to stage this state).
    Object.assign(ctx.agents.get(source.id) as object, { status: 'running' })

    const response = await api(ctx).sessions.fork(request({ sessionId: source.id, beforeSeq: 1 }))

    expect(response.result.ok).toBe(true)
    if (!response.result.ok) return
    expect(response.result.value.restoreReport).toMatchObject({
      restored: 0,
      skipped: 'source-running',
    })
    await ctx.fiber.dispose()
  })
})
