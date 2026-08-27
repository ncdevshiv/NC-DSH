/**
 * Pure translation coverage: harness messages to sidecar wire requests, and
 * sidecar events to correlated harness chunks.
 * @module translate.spec
 */

import { describe, expect, it } from 'vitest'
import { CallId, MessageId, ReasoningEffortId } from '@deepseek-ai/dsh-llm'
import type { Message } from '@deepseek-ai/dsh-llm'
import { finishReasonOf, StreamAssembler, toChatRequest } from '../src/translate.ts'

function message(input: Omit<Message, 'id'>): Message {
  return { ...input, id: MessageId('test') }
}

describe('toChatRequest', () => {
  it('prepends the system slot and flattens text blocks', () => {
    const request = toChatRequest({
      provider: 'deepseek-official',
      model: 'deepseek-v4-flash',
      system: 'be brief',
      messages: [
        message({ role: 'user', content: [{ type: 'text', text: 'hello' }], source: { kind: 'user' } }),
      ],
    }, new Map())
    expect(request.messages).toEqual([
      { role: 'system', parts: [{ type: 'text', text: 'be brief' }] },
      { role: 'user', parts: [{ type: 'text', text: 'hello' }] },
    ])
  })

  it('maps tool results to the tool role and carries correlation', () => {
    const request = toChatRequest({
      provider: 'p', model: 'm',
      messages: [
        message({
          role: 'assistant',
          content: [{ type: 'tool-call', id: CallId('call-1'), name: 'lookup', arguments: '{"q":1}' }],
          source: { kind: 'model', provider: 'p', model: 'm' },
        }),
        message({
          role: 'user',
          content: [{
            type: 'tool-result',
            toolCallId: CallId('call-1'),
            content: [{ type: 'text', text: 'result text' }],
            isError: false,
          }],
          source: { kind: 'tool', callId: CallId('call-1') },
        }),
      ],
    }, new Map())
    expect(request.messages[0]).toEqual({
      role: 'assistant',
      parts: [{ type: 'tool_call', call: { id: 'call-1', name: 'lookup', arguments: '{"q":1}' } }],
    })
    expect(request.messages[1]).toEqual({
      role: 'tool',
      parts: [{ type: 'tool_result', result: { id: 'call-1', name: '', output: 'result text', is_error: false } }],
    })
  })

  it('rejects unresolved image blocks loudly', () => {
    expect(() => toChatRequest({
      provider: 'p', model: 'm',
      messages: [message({
        role: 'user',
        content: [{
          type: 'image',
          attachment: {
            attachmentId: 'a-1' as never,
            mediaType: 'image/png' as never,
            bytes: 1, width: 1, height: 1,
          },
        }],
        source: { kind: 'user' },
      })],
    }, new Map())).toThrow(/not pre-resolved/)
  })

  it('projects reasoning efforts and request knobs onto the wire body', () => {
    const request = toChatRequest({
      provider: 'p', model: 'm',
      reasoningEffort: ReasoningEffortId('max'),
      temperature: 0.5,
      maxTokens: 128,
      stop: ['END'],
      tools: [{ name: 't', description: 'd', parameters: { type: 'object' } }],
      messages: [message({ role: 'user', content: [{ type: 'text', text: 'hi' }], source: { kind: 'user' } })],
    }, new Map())
    expect(request.reasoning_effort).toBe('max')
    expect(request.temperature).toBe(0.5)
    expect(request.max_tokens).toBe(128)
    expect(request.stop).toEqual(['END'])
    expect(request.tools).toEqual([{ name: 't', description: 'd', input_schema: { type: 'object' } }])
  })

  it('omits the reasoning effort for off so the provider default applies', () => {
    const request = toChatRequest({
      provider: 'p', model: 'm', reasoningEffort: ReasoningEffortId('off'),
      messages: [message({ role: 'user', content: [{ type: 'text', text: 'hi' }], source: { kind: 'user' } })],
    }, new Map())
    expect(request.reasoning_effort).toBeUndefined()
  })
})

describe('StreamAssembler', () => {
  it('opens, deltas, and closes a text block from flat deltas', () => {
    const assembler = new StreamAssembler()
    expect(assembler.accept({ type: 'text_delta', delta: 'he' })).toEqual([
      { type: 'block-start', index: 0, blockType: 'text' },
      { type: 'text-delta', index: 0, text: 'he' },
    ])
    expect(assembler.accept({ type: 'text_delta', delta: 'y' })).toEqual([
      { type: 'text-delta', index: 0, text: 'y' },
    ])
    expect(assembler.accept({ type: 'completed', finish_reason: 'stop' })).toEqual([
      { type: 'block-end', index: 0, block: { type: 'text', text: 'hey' } },
    ])
    expect(assembler.close()).toEqual([])
  })

  it('splits interleaved reasoning and text into separate indexed blocks', () => {
    const assembler = new StreamAssembler()
    const chunks = [
      ...assembler.accept({ type: 'reasoning_delta', delta: 'think' }),
      ...assembler.accept({ type: 'text_delta', delta: 'say' }),
      ...assembler.close(),
    ]
    expect(chunks).toEqual([
      { type: 'block-start', index: 0, blockType: 'reasoning' },
      { type: 'reasoning-delta', index: 0, text: 'think' },
      { type: 'block-end', index: 0, block: { type: 'reasoning', text: 'think' } },
      { type: 'block-start', index: 1, blockType: 'text' },
      { type: 'text-delta', index: 1, text: 'say' },
      { type: 'block-end', index: 1, block: { type: 'text', text: 'say' } },
    ])
  })

  it('correlates tool-call frames into one authoritative block-end', () => {
    const assembler = new StreamAssembler()
    const chunks = [
      ...assembler.accept({ type: 'tool_call_started', id: 'c1', name: 'lookup' }),
      ...assembler.accept({ type: 'tool_call_delta', id: 'c1', arguments_delta: '{"q"' }),
      ...assembler.accept({ type: 'tool_call_delta', id: 'c1', arguments_delta: ':1}' }),
      ...assembler.accept({ type: 'tool_call_completed', call: { id: 'c1', name: 'lookup', arguments: '{"q":1}' } }),
    ]
    expect(chunks).toEqual([
      { type: 'block-start', index: 0, blockType: 'tool-call' },
      { type: 'tool-call-delta', id: CallId('c1'), name: 'lookup', argumentsDelta: '' },
      { type: 'tool-call-delta', id: CallId('c1'), argumentsDelta: '{"q"' },
      { type: 'tool-call-delta', id: CallId('c1'), argumentsDelta: ':1}' },
      { type: 'block-end', index: 0, block: { type: 'tool-call', id: CallId('c1'), name: 'lookup', arguments: '{"q":1}' } },
    ])
  })

  it('defers usage to stream end so it never lands inside an open block', () => {
    const assembler = new StreamAssembler()
    expect(assembler.accept({
      type: 'usage_update',
      usage: { input_tokens: 100, output_tokens: 20, cached_input_tokens: 30, reasoning_tokens: 5 },
    })).toEqual([])
    // Whatever order the provider attaches usage to its terminal content
    // event, assembly emits block-end first and usage before any finish.
    const chunks = [
      ...assembler.accept({ type: 'completed' }),
      ...assembler.close(),
    ]
    expect(chunks).toEqual([{
      type: 'usage',
      usage: { inputTokens: 70, outputTokens: 20, cacheReadTokens: 30, reasoningTokens: 5 },
    }])
  })

  it('surfaces a mid-stream error note as reasoning without faking a finish', () => {
    const assembler = new StreamAssembler()
    const chunks = [
      ...assembler.accept({ type: 'error', message: 'provider hiccup' }),
      ...assembler.close(),
    ]
    expect(chunks.at(-1)).toEqual({ type: 'block-end', index: 0, block: { type: 'reasoning', text: 'provider hiccup' } })
    expect(chunks.some(chunk => chunk.type === 'finish')).toBe(false)
  })
})

describe('finishReasonOf', () => {
  it('maps the recognized vocabulary and falls through unknowns to stop', () => {
    expect(finishReasonOf('stop')).toEqual({ kind: 'stop' })
    expect(finishReasonOf('tool_calls')).toEqual({ kind: 'tool-calls' })
    expect(finishReasonOf('length')).toEqual({ kind: 'max-tokens' })
    expect(finishReasonOf(undefined)).toEqual({ kind: 'stop' })
    expect(finishReasonOf('content_filter')).toEqual({ kind: 'stop' })
  })
})
