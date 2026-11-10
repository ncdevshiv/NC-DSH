/**
 * The one reading of a connectivity probe's chunk stream.
 *
 * A probe is a real model request with a one-line prompt: it settles at the
 * first model output, at the provider's terminal chunk, or at the first
 * failure, and the harness stops reading. Settling at the first chunk is what
 * makes the call cheap enough to run from a settings page; the probe still
 * reaches the provider over the same adapter chain a conversation request
 * uses, so the answer is the provider's, not a locally simulated verdict.
 *
 * @module @deepseek-ai/dsh-llm/model-probe
 */

import { normalizeLlmFailure } from './adapter-failure.ts'
import { createUserMessage } from './message.ts'
import type { LlmFailure, LlmModelTestResult, RequestMessage, StreamChunk, TokenUsage } from './types.ts'

/** Machine code for a provider response that closed without reaching a terminal chunk. */
const INCOMPLETE_RESPONSE_CODE = 'INCOMPLETE_RESPONSE'

/** Prompt every connectivity probe sends. Short because the answer only has to begin. */
const PROBE_PROMPT = 'Connectivity check. Reply with the single word: ok.'

/**
 * The one user message a connectivity probe puts in front of the model.
 * @returns a fresh user-role message carrying the probe prompt.
 */
export function probeMessages(): RequestMessage[] {
  return [createUserMessage({ content: [{ type: 'text', text: PROBE_PROMPT }], source: { kind: 'user' } })]
}

/**
 * Drive one probe request to its measured outcome.
 * @param probe - the model under test, the caller's cancellation, and the request opener.
 * @param probe.model - exact model id the probe addresses.
 * @param probe.signal - caller cancellation; it reports `ABORTED`, never a provider rejection.
 * @param probe.open - dispatches the request, called once with the probe's own
 *   cancellation signal; a throw or rejection is the probe's failure.
 * @returns `ok` with the measured latency and any usage the provider reported,
 *   or `ok: false` with that provider's failure facts. Never throws: a refused
 *   key, model id, or request is the answer the caller asked for.
 */
export async function settleModelProbe(probe: {
  model: string
  signal?: AbortSignal
  open: (signal: AbortSignal) => AsyncIterable<StreamChunk> | Promise<AsyncIterable<StreamChunk>>
}): Promise<LlmModelTestResult> {
  const started = performance.now()
  // One controller owns the probe's own teardown: the caller's signal reports
  // cancellation to the user, while this one cancels the provider request the
  // moment the probe has its answer, before the response is drained.
  const consumer = new AbortController()
  const signal = probe.signal === undefined
    ? consumer.signal
    : AbortSignal.any([probe.signal, consumer.signal])
  const elapsed = (): number => Math.round(performance.now() - started)
  const answered = (usage: TokenUsage | undefined): LlmModelTestResult => ({
    ok: true,
    model: probe.model,
    latencyMs: elapsed(),
    ...usage === undefined ? {} : { usage },
  })
  const refused = (failure: LlmFailure): LlmModelTestResult => ({
    ok: false,
    model: probe.model,
    latencyMs: elapsed(),
    failure,
  })
  const aborted = (): LlmModelTestResult => refused({ message: 'model test aborted by caller', code: 'ABORTED' })
  let usage: TokenUsage | undefined
  try {
    for await (const chunk of await probe.open(signal)) {
      if (chunk.type === 'usage') {
        // pi-ai reports usage on its error delivery too, so this chunk alone
        // does not answer the probe; it is recorded for whichever chunk does.
        usage = chunk.usage
        continue
      }
      if (chunk.type === 'text-delta'
        || chunk.type === 'reasoning-delta'
        || chunk.type === 'tool-call-delta') {
        return answered(usage)
      }
      if (chunk.type === 'finish') {
        const reason = chunk.reason
        if (reason.kind === 'error' || reason.kind === 'aborted') {
          return probe.signal?.aborted === true ? aborted() : refused(reason.failure)
        }
        return answered(usage)
      }
    }
    return probe.signal?.aborted === true
      ? aborted()
      : refused({ message: 'the provider closed the response without answering', code: INCOMPLETE_RESPONSE_CODE })
  } catch (error: unknown) {
    return probe.signal?.aborted === true ? aborted() : refused(normalizeLlmFailure(error))
  } finally {
    consumer.abort('model probe settled')
  }
}
