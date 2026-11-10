import { describe, expect, it, vi } from 'vitest'
import { runAgent } from '../src/loop.ts'
import type { AgentRunRequest, LoopHooks } from '../src/loop.ts'
import type { AgentToolCall, LlmPort, LlmRequest, LlmResponse } from '../src/ports.ts'
import type { RuntimeTool } from '../src/tools.ts'

function response(overrides: Partial<LlmResponse> = {}): LlmResponse {
  return { text: '', toolCalls: [], usage: { inputTokens: 1, outputTokens: 2, cachedTokens: 3 }, finishReason: 'stop', ...overrides }
}

function portOf(responses: readonly LlmResponse[]): LlmPort {
  const queue = [...responses]
  return {
    complete: (request: LlmRequest) => {
      void request
      const next = queue.shift() ?? response()
      return Promise.resolve(next)
    },
  }
}

function call(name: string, argumentsJson: string, id = `call_${name}`): AgentToolCall {
  return { id, name, argumentsJson }
}

function hooks(): LoopHooks & { toolResults: { text: string; isError: boolean }[] } {
  const toolResults: { text: string; isError: boolean }[] = []
  return {
    toolResults,
    beforeRequest: vi.fn(),
    afterResponse: vi.fn(),
    onToolCall: vi.fn(),
    onToolResult: (_call, outcome) => { toolResults.push(outcome) },
  }
}

function runtimeTool(execute: RuntimeTool['execute']): RuntimeTool {
  return {
    schema: { name: 'read', description: 'd', parameters: { type: 'object', required: ['path'], properties: { path: { type: 'string' } } } },
    execute,
  }
}

function request(overrides: Partial<AgentRunRequest> = {}): AgentRunRequest {
  return {
    agentId: 'agt_1',
    taskId: null,
    projectId: 'prj_1',
    workspacePath: '/tmp/ws',
    provider: 'p',
    model: 'm',
    system: 'system',
    messages: [{ role: 'user', text: 'do it' }],
    tools: [],
    maxSteps: 4,
    maxTokens: 100,
    signal: new AbortController().signal,
    hooks: hooks(),
    ...overrides,
  }
}

describe('ncag agent loop', () => {
  it('returns a final answer after one call and accumulates usage', async () => {
    const run = request({ hooks: hooks() })
    const result = await runAgent(portOf([response({ text: ' done ' })]), run)
    expect(result.completed).toBe(true)
    expect(result.finalText).toBe('done')
    expect(result.steps).toBe(1)
    expect(result.usage).toEqual({ inputTokens: 1, outputTokens: 2, cachedTokens: 3 })
    expect(result.transcript.at(-1)).toEqual({ role: 'assistant', text: 'done' })
  })

  it('completes with no text when the model returns nothing', async () => {
    const result = await runAgent(portOf([response()]), request())
    expect(result.completed).toBe(true)
    expect(result.finalText).toBe('')
    expect(result.transcript).toHaveLength(1)
  })

  it('executes a tool call and feeds the result back', async () => {
    const runHooks = hooks()
    const execute = vi.fn(() => ({ text: 'file body', isError: false }))
    const run = request({ tools: [runtimeTool(execute)], hooks: runHooks })
    const result = await runAgent(portOf([
      response({ toolCalls: [call('read', '{"path":"a.txt"}')] }),
      response({ text: 'finished' }),
    ]), run)
    expect(execute).toHaveBeenCalledTimes(1)
    expect(runHooks.toolResults).toEqual([{ text: 'file body', isError: false }])
    expect(result.transcript.find(message => message.role === 'tool')).toEqual({
      role: 'tool', text: 'file body', toolCallId: 'call_read', toolName: 'read', isError: false,
    })
    expect(result.completed).toBe(true)
  })

  it('reports malformed, unknown-tool and missing-argument calls without executing', async () => {
    const execute = vi.fn(() => ({ text: 'should not run', isError: false }))
    const runHooks = hooks()
    const result = await runAgent(portOf([
      response({ toolCalls: [
        call('read', '{not json'),
        call('nope', '{}'),
        call('read', '{}'),
        call('read', '5'),
      ] }),
      response({ text: 'ok' }),
    ]), request({ tools: [runtimeTool(execute)], hooks: runHooks }))
    expect(execute).not.toHaveBeenCalled()
    expect(runHooks.toolResults.map(outcome => outcome.isError)).toEqual([true, true, true, true])
    expect(runHooks.toolResults[0]?.text).toContain('invalid tool arguments')
    expect(runHooks.toolResults[1]?.text).toContain('unknown tool')
    expect(runHooks.toolResults[2]?.text).toContain('missing required argument')
    expect(runHooks.toolResults[3]?.text).toContain('arguments must be a JSON object')
    expect(result.completed).toBe(true)
  })

  it('executes a tool whose schema declares no required arguments', async () => {
    const execute = vi.fn(() => ({ text: 'ran', isError: false }))
    const openTool: RuntimeTool = {
      schema: { name: 'read', description: 'd', parameters: { type: 'object' } },
      execute,
    }
    const runHooks = hooks()
    await runAgent(portOf([
      response({ toolCalls: [call('read', '{}')] }),
      response({ text: 'done' }),
    ]), request({ tools: [openTool], hooks: runHooks }))
    expect(execute).toHaveBeenCalledTimes(1)
  })

  it('reports a thrown tool error as a failed outcome', async () => {
    const execute = vi.fn(() => { throw new Error('disk on fire') })
    const runHooks = hooks()
    await runAgent(portOf([
      response({ toolCalls: [call('read', '{"path":"a.txt"}')] }),
      response({ text: 'done' }),
    ]), request({ tools: [runtimeTool(execute)], hooks: runHooks }))
    expect(runHooks.toolResults).toEqual([{ text: 'tool failed: disk on fire', isError: true }])
  })

  it('stops without completing when the step budget is exhausted', async () => {
    const run = request({ maxSteps: 2, tools: [runtimeTool(() => ({ text: 'again', isError: false }))] })
    const result = await runAgent(portOf([
      response({ toolCalls: [call('read', '{"path":"a"}')] }),
      response({ toolCalls: [call('read', '{"path":"a"}')] }),
    ]), run)
    expect(result.completed).toBe(false)
    expect(result.steps).toBe(2)
  })

  it('refuses to run after the signal is aborted', async () => {
    const controller = new AbortController()
    controller.abort()
    await expect(runAgent(portOf([response()]), request({ signal: controller.signal }))).rejects.toThrow(/aborted/)
  })
})
