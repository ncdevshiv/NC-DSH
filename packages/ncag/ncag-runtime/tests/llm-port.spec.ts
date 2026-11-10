import { describe, expect, it } from 'vitest'
import { brandString } from '@deepseek-ai/dsh-brand'
import type { GenerateOptions, ToolCallId, StreamChunk } from '@deepseek-ai/dsh-llm'
import { createLlmPort } from '../src/llm-port.ts'
import type { LlmFacade } from '../src/llm-port.ts'
import type { LlmRequest } from '../src/ports.ts'

async function* streamOf(chunks: readonly StreamChunk[]): AsyncIterable<StreamChunk> {
  for (const chunk of chunks) yield chunk
}

function facade(chunks: readonly StreamChunk[]): LlmFacade {
  return {
    stream: () => streamOf(chunks),
    listProviders: () => [{ id: 'p', name: 'P' }],
    listModels: () => Promise.resolve([{ id: 'm', name: 'M' }]),
  }
}

function request(overrides: Partial<LlmRequest> = {}): LlmRequest {
  return {
    provider: 'p',
    model: 'm',
    system: 'system',
    messages: [],
    tools: [],
    maxTokens: 100,
    signal: new AbortController().signal,
    ...overrides,
  }
}

describe('ncag production model port', () => {
  it('assembles text, usage and a stop reason from the chunk stream', async () => {
    const port = createLlmPort(facade([
      { type: 'block-start', index: 0, blockType: 'text' },
      { type: 'text-delta', index: 0, text: 'hello ' },
      { type: 'text-delta', index: 0, text: 'world' },
      { type: 'reasoning-delta', index: 1, text: 'thinking' },
      { type: 'usage', usage: { inputTokens: 3, outputTokens: 4, cacheReadTokens: 5 } },
      { type: 'finish', reason: { kind: 'stop' } },
    ]))
    const response = await port.complete(request())
    expect(response.text).toBe('hello world')
    expect(response.toolCalls).toEqual([])
    expect(response.usage).toEqual({ inputTokens: 3, outputTokens: 4, cachedTokens: 5 })
    expect(response.finishReason).toBe('stop')
  })

  it('defaults cached tokens to zero when the provider reports none', async () => {
    const port = createLlmPort(facade([
      { type: 'usage', usage: { inputTokens: 1, outputTokens: 2 } },
      { type: 'finish', reason: { kind: 'tool-calls' } },
    ]))
    const response = await port.complete(request())
    expect(response.usage.cachedTokens).toBe(0)
    expect(response.finishReason).toBe('tool-calls')
  })

  it('assembles tool calls from deltas and from a completed block', async () => {
    const port = createLlmPort(facade([
      { type: 'tool-call-delta', index: 0, id: brandString<ToolCallId>('call_1'), name: 'read', argumentsDelta: '{"path":' },
      { type: 'tool-call-delta', index: 0, id: brandString<ToolCallId>('call_1'), argumentsDelta: '"a.txt"}' },
      { type: 'tool-call-delta', index: 1, id: brandString<ToolCallId>('call_2'), argumentsDelta: '{"path":"b.txt"}' },
      { type: 'block-end', index: 1, block: { type: 'tool-call', id: brandString<ToolCallId>('call_2'), name: 'read', arguments: '{"path":"b.txt"}' } },
      // A completed non-tool block is ignored by the tool assembler.
      { type: 'block-end', index: 2, block: { type: 'text', text: 'ignored' } },
      { type: 'finish', reason: { kind: 'tool-calls' } },
    ]))
    const response = await port.complete(request())
    expect(response.toolCalls).toEqual([
      { id: 'call_1', name: 'read', argumentsJson: '{"path":"a.txt"}' },
      { id: 'call_2', name: 'read', argumentsJson: '{"path":"b.txt"}' },
    ])
  })

  it('forwards reasoning effort and maps every message role', async () => {
    let captured: GenerateOptions | undefined
    const llm: LlmFacade = {
      stream: (options) => {
        captured = options
        return streamOf([{ type: 'finish', reason: { kind: 'stop' } }])
      },
      listProviders: () => [],
      listModels: () => Promise.resolve([]),
    }
    const port = createLlmPort(llm)
    await port.complete(request({
      reasoningEffort: 'high',
      tools: [{ name: 'read', description: 'd', parameters: { type: 'object' } }],
      messages: [
        { role: 'user', text: 'u' },
        { role: 'assistant', text: 'a' },
        { role: 'tool', text: 't', toolCallId: 'call_1', isError: true },
        { role: 'tool', text: 'no call id', isError: false },
        { role: 'system', text: 'ignored by the mapper' },
      ],
    }))
    expect(captured?.reasoningEffort).toBe('high')
    expect(captured?.messages).toHaveLength(4)
    expect(captured?.tools).toEqual([{ name: 'read', description: 'd', parameters: { type: 'object' } }])
  })
})
