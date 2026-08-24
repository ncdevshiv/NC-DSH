import { createUserMessage } from '@deepseek-ai/dsh-llm'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { chmodSync, existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { Context } from '@deepseek-ai/cordis'
import { SessionId, type SessionEvent } from '@deepseek-ai/dsh-session'
import type { Agent } from '@deepseek-ai/dsh-agent'
import AgentLoop from '@deepseek-ai/dsh-agent-loop'
import { mountAgentLoopTestDependencies } from '@deepseek-ai/dsh-agent-loop-testkit'
import { LocalBashExecutor } from '@deepseek-ai/dsh-bash-local'
import LocalSubprocessRuntime from '@deepseek-ai/dsh-subprocess-local'
import * as HooksCodex from '@deepseek-ai/dsh-hooks-codex'
import { MockAdapter, textResponse } from '../../../core/agent-loop/tests/mock-adapter.ts'

/**
 * The Stop-hook loop guard, Codex dialect: an always-blocking Stop hook must
 * stop extending a turn once `maxConsecutiveStopBlocks` continuations are
 * spent, the payload's `stop_hook_active` must report the continuation chain
 * honestly, and each new turn must start with a fresh budget.
 */

const dirs: string[] = []
afterEach(() => { for (const d of dirs.splice(0)) rmSync(d, { recursive: true, force: true }) })

function dir(): string {
  const d = mkdtempSync(join(tmpdir(), 'dsh-hooks-codex-guard-'))
  dirs.push(d)
  return d
}
function script(d: string, name: string, body: string): string {
  const path = join(d, name)
  writeFileSync(path, body)
  chmodSync(path, 0o755)
  return path
}
function writeHooks(d: string, hooks: unknown): void {
  writeFileSync(join(d, 'hooks.json'), JSON.stringify({ hooks }))
}

async function harness(d: string, adapter: MockAdapter, maxConsecutiveStopBlocks?: number): Promise<Context> {
  const ctx = new Context()
  await mountAgentLoopTestDependencies(ctx)
  await ctx.plugin(AgentLoop, { agents: [] })
  await ctx.plugin(LocalSubprocessRuntime)
  await ctx.plugin(LocalBashExecutor, { timeoutMs: 10_000 })
  await ctx.plugin(HooksCodex, {
    configPath: join(d, 'hooks.json'),
    model: 'test-model',
    ...maxConsecutiveStopBlocks !== undefined ? { maxConsecutiveStopBlocks } : {},
  })
  ctx.llm.registerAdapter(['mock'], adapter)
  return ctx
}
function waitForIdle(_ctx: Context, agent: Agent): Promise<void> {
  return agent.whenIdle()
}
function events(agent: Agent): SessionEvent[] { return [...agent.session.events] }
function followup(agent: Agent, text: string): void {
  agent.followup(createUserMessage({ content: [{ type: 'text', text }], source: { kind: 'user' } }))
}

describe('hooks-codex Stop loop guard', () => {
  it('an always-blocking Stop hook stops extending the turn at the cap', async () => {
    const d = dir()
    // Blocks unconditionally — without the cap this never terminates.
    const s = script(d, 'stop.sh', '#!/usr/bin/env bash\necho "not done" >&2\nexit 2\n')
    writeHooks(d, { Stop: [{ hooks: [{ type: 'command', command: s }] }] })
    const adapter = new MockAdapter([textResponse('one'), textResponse('two'), textResponse('three')])
    const ctx = await harness(d, adapter, 2)
    const warn = vi.fn(); ctx.logger.warn = warn as never
    const agent = ctx.agentLoop.create(SessionId('cap'), { provider: 'mock', model: 'mock' })
    followup(agent, 'go')
    await waitForIdle(ctx, agent)

    // Initial request plus exactly `cap` forced continuations, then the turn closed.
    expect(adapter.requests).toHaveLength(3)
    expect(warn).toHaveBeenCalledWith(expect.stringContaining('maxConsecutiveStopBlocks'))
    expect(events(agent).filter(e => e.type === 'hook/result')).toHaveLength(3)
    expect(events(agent).at(-1)?.type).toBe('turn/end')
  }, 20_000)

  it('reports stop_hook_active honestly across the continuation chain', async () => {
    const d = dir()
    const log = join(d, 'payloads.ndjson')
    // Codex writes stdin WITHOUT a trailing newline; normalize before appending.
    const s = script(d, 'stop.sh', `#!/usr/bin/env bash\ncat >> "${log}"\necho >> "${log}"\necho "again" >&2\nexit 2\n`)
    writeHooks(d, { Stop: [{ hooks: [{ type: 'command', command: s }] }] })
    const adapter = new MockAdapter([textResponse('one'), textResponse('two'), textResponse('three')])
    const ctx = await harness(d, adapter, 2)
    const agent = ctx.agentLoop.create(SessionId('flag'), { provider: 'mock', model: 'mock' })
    followup(agent, 'go')
    await waitForIdle(ctx, agent)

    expect(existsSync(log)).toBe(true)
    const flags = readFileSync(log, 'utf8').trim().split('\n')
      .map(line => (JSON.parse(line) as { stop_hook_active: boolean }).stop_hook_active)
    expect(flags).toEqual([false, true, true])
  }, 20_000)

  it('each new turn starts with a fresh block budget', async () => {
    const d = dir()
    const s = script(d, 'stop.sh', '#!/usr/bin/env bash\necho "more" >&2\nexit 2\n')
    writeHooks(d, { Stop: [{ hooks: [{ type: 'command', command: s }] }] })
    // Cap 1: every turn gets its initial request plus one continuation.
    const adapter = new MockAdapter([
      textResponse('t1-a'), textResponse('t1-b'),
      textResponse('t2-a'), textResponse('t2-b'),
    ])
    const ctx = await harness(d, adapter, 1)
    const warn = vi.fn(); ctx.logger.warn = warn as never
    const agent = ctx.agentLoop.create(SessionId('fresh'), { provider: 'mock', model: 'mock' })
    followup(agent, 'first')
    await waitForIdle(ctx, agent)
    followup(agent, 'second')
    await waitForIdle(ctx, agent)

    // A stale budget would have absorbed the second turn's first block.
    expect(adapter.requests).toHaveLength(4)
    expect(warn).toHaveBeenCalledTimes(2)
  }, 30_000)

  it('rejects a non-positive or fractional maxConsecutiveStopBlocks at load', async () => {
    const d = dir()
    writeHooks(d, {})
    for (const bad of [0, -5, 1.5]) {
      const adapter = new MockAdapter([])
      await expect(harness(d, adapter, bad))
        .rejects.toThrow(/hooks-codex: maxConsecutiveStopBlocks must be a positive integer/)
    }
  })
})
