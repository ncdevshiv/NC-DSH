import { describe, expect, it } from 'vitest'
import { LlmError, ProviderRequestId, ToolCallId } from '@deepseek-ai/dsh-llm'
import type { LlmFailure, StreamChunk } from '@deepseek-ai/dsh-llm'
import { probeMessages, settleModelProbe } from '../src/model-probe.ts'

/** Scripted provider output, yielded in order. */
async function* chunks(...values: StreamChunk[]): AsyncGenerator<StreamChunk> {
  yield* values
}

const usage: StreamChunk = { type: 'usage', usage: { inputTokens: 12, outputTokens: 3 } }

/** A terminal chunk that reports a completed answer. */
function stopped(): StreamChunk {
  return { type: 'finish', reason: { kind: 'stop' } }
}

/** A terminal chunk that reports the provider's own refusal or cancellation. */
function finished(kind: 'error' | 'aborted', failure: LlmFailure): StreamChunk {
  return { type: 'finish', reason: { kind, failure } }
}

describe('connectivity probe settlement', () => {
  it('settles at the first model output and reports a measured latency', async () => {
    const result = await settleModelProbe({
      model: 'acme-large',
      open: () => chunks(
        { type: 'block-start', index: 0, blockType: 'text' },
        { type: 'text-delta', index: 0, text: 'ok' },
        usage,
        stopped(),
      ),
    })
    expect(result).toMatchObject({ ok: true, model: 'acme-large' })
    expect(Number.isInteger(result.latencyMs)).toBe(true)
    // The probe answered before the provider reported usage, so none is claimed.
    expect(result.usage).toBeUndefined()
    expect(result.failure).toBeUndefined()
  })

  it.each([
    { type: 'reasoning-delta', index: 0, text: 'thinking' },
    { type: 'tool-call-delta', index: 0, id: ToolCallId('call-1'), argumentsDelta: '{}' },
  ] as const satisfies readonly StreamChunk[])('treats a %s chunk as an answer', async (chunk) => {
    const result = await settleModelProbe({ model: 'm', open: () => chunks(chunk, usage, stopped()) })
    expect(result.ok).toBe(true)
  })

  it('reports the usage a provider sends before its terminal chunk', async () => {
    const result = await settleModelProbe({ model: 'm', open: () => chunks(usage, stopped()) })
    expect(result).toEqual({
      ok: true,
      model: 'm',
      latencyMs: result.latencyMs,
      usage: { inputTokens: 12, outputTokens: 3 },
    })
  })

  it('accepts a terminal chunk with no usage report', async () => {
    const result = await settleModelProbe({
      model: 'm',
      open: () => chunks({ type: 'block-end', index: 0, block: { type: 'text', text: 'ok' } }, stopped()),
    })
    expect(result.ok).toBe(true)
    expect(result.usage).toBeUndefined()
  })

  it('reports the provider refusal rather than inventing one', async () => {
    const failure: LlmFailure = { message: 'Invalid API key', code: 'AUTH', status: 401 }
    const result = await settleModelProbe({ model: 'm', open: () => chunks(finished('error', failure)) })
    expect(result).toEqual({ ok: false, model: 'm', latencyMs: result.latencyMs, failure })
    expect(result.latencyMs).toBeGreaterThanOrEqual(0)
  })

  it('reports an aborted stream as cancellation, and only as cancellation once the caller cancelled', async () => {
    const failure: LlmFailure = { message: 'pi-ai stream aborted', code: 'ABORTED' }
    const providerSide = await settleModelProbe({ model: 'm', open: () => chunks(finished('aborted', failure)) })
    expect(providerSide.failure).toEqual(failure)

    const controller = new AbortController()
    controller.abort('user closed the dialog')
    const callerSide = await settleModelProbe({ model: 'm', signal: controller.signal, open: () => chunks(finished('aborted', failure)) })
    expect(callerSide.failure).toEqual({ message: 'model test aborted by caller', code: 'ABORTED' })
  })

  it('reports a response that closed without answering', async () => {
    const result = await settleModelProbe({ model: 'm', open: () => chunks({ type: 'block-start', index: 0, blockType: 'text' }) })
    expect(result).toMatchObject({ ok: false, failure: { code: 'INCOMPLETE_RESPONSE' } })
  })

  it('reports caller cancellation over a response that never terminalized', async () => {
    const controller = new AbortController()
    controller.abort()
    const result = await settleModelProbe({ model: 'm', signal: controller.signal, open: () => chunks() })
    expect(result.failure).toEqual({ message: 'model test aborted by caller', code: 'ABORTED' })
  })

  it('reports a request that failed before any chunk', async () => {
    const result = await settleModelProbe({
      model: 'm',
      open: () => {
        throw new LlmError('no adapter registered for provider "acme"', 'NO_ADAPTER', { status: 503, requestId: ProviderRequestId('req-1') })
      },
    })
    expect(result).toMatchObject({
      ok: false,
      failure: { message: 'no adapter registered for provider "acme"', code: 'NO_ADAPTER', status: 503, requestId: 'req-1' },
    })
  })

  it('reports a failure thrown partway through the response', async () => {
    async function* broken(): AsyncGenerator<StreamChunk> {
      yield usage
      throw new Error('socket closed')
    }
    const result = await settleModelProbe({ model: 'm', open: broken })
    expect(result).toMatchObject({ ok: false, failure: { message: 'socket closed', code: 'UNKNOWN' } })
  })

  it('reports caller cancellation for a request that failed while cancelling', async () => {
    const controller = new AbortController()
    controller.abort('gone')
    const result = await settleModelProbe({
      model: 'm',
      signal: controller.signal,
      open: signal => {
        signal.throwIfAborted()
        return chunks(stopped())
      },
    })
    expect(result.failure).toEqual({ message: 'model test aborted by caller', code: 'ABORTED' })
  })

  it('cancels the provider request once the probe has its answer', async () => {
    let observed: AbortSignal | undefined
    const controller = new AbortController()
    const result = await settleModelProbe({
      model: 'm',
      signal: controller.signal,
      open: signal => {
        observed = signal
        return chunks(usage, stopped())
      },
    })
    expect(result.ok).toBe(true)
    // The probe owns its own cancellation: the caller stayed live, while the
    // request behind the settled answer is cancelled.
    expect(controller.signal.aborted).toBe(false)
    expect(observed?.aborted).toBe(true)
  })

  it('carries an already cancelled caller into the probe signal', async () => {
    const controller = new AbortController()
    controller.abort('gone')
    let observed: AbortSignal | undefined
    const result = await settleModelProbe({
      model: 'm',
      signal: controller.signal,
      open: signal => {
        observed = signal
        return chunks()
      },
    })
    expect(observed?.aborted).toBe(true)
    expect(result.ok).toBe(false)
  })
})

describe('connectivity probe input', () => {
  it('sends one user message that asks only for an answer', () => {
    const [message] = probeMessages()
    expect(message?.role).toBe('user')
    const [block] = message?.content ?? []
    expect(block?.type).toBe('text')
    expect(block?.type === 'text' && block.text.length > 0).toBe(true)
  })
})
