/**
 * Shared non-2xx response-body handling for web providers.
 *
 * Every search provider repeats the same dance on a non-2xx response: read
 * the body, try JSON, extract a human-readable message from whichever
 * envelope the gateway used, and keep the HTTP status as the fallback line.
 * This module owns that once so provider failures report consistently:
 * the same bounded read (a hostile megabyte body cannot inflate one failure
 * message), the same envelope shapes (OpenAI `error.message`, Anthropic-style
 * top-level `message`, plain `detail`/`error` strings), and aborts surfacing
 * as cancellation rather than being swallowed into an HTTP-error message.
 *
 * @module @deepseek-ai/dsh-web/error-body
 */

import { WebError } from './types.ts'

/** Largest error body any web provider reads on a non-2xx response. */
export const MAX_WEB_ERROR_BODY_CHARS = 16 * 1024

/**
 * Structured facts extracted from one non-2xx error body.
 * @property message - best human-readable line found; empty when none.
 * @property providerType - provider's structural error type when the body names one.
 */
export interface ParsedErrorBody {
  readonly message: string
  readonly providerType?: string
}

/**
 * Outcome of reading one non-2xx response body.
 * - `parsed`: facts extracted from the body (`truncated` reports a cap cut).
 * - `aborted`: caller cancellation fired mid-read; the provider maps this to
 *   its own `WEB_ABORTED` failure so cancellation never masquerades as a
 *   provider error (the seam's cancellation contract).
 */
export type ErrorBodyRead =
  | ({ readonly kind: 'parsed'; readonly truncated: boolean } & ParsedErrorBody)
  | { readonly kind: 'aborted'; readonly cause: unknown }

/**
 * Read and parse a non-2xx response body. The read is character-capped so a
 * broken or hostile endpoint cannot exhaust memory through its error path;
 * an over-cap body is cancelled mid-stream and marked truncated.
 * @param response - the non-2xx response whose body to read.
 * @returns the parsed facts, or the abort cause when cancellation fired
 *   mid-read.
 */
export async function readErrorBody(response: Response): Promise<ErrorBodyRead> {
  if (response.body === null) return { kind: 'parsed', message: '', truncated: false }
  const reader = response.body.getReader()
  const decoder = new TextDecoder()
  let text = ''
  try {
    for (;;) {
      const { done, value } = await reader.read()
      if (done) break
      text += decoder.decode(value, { stream: true })
      if (text.length >= MAX_WEB_ERROR_BODY_CHARS) {
        await reader.cancel('truncated by harness error-body cap')
        text = text.slice(0, MAX_WEB_ERROR_BODY_CHARS)
        return { kind: 'parsed', ...parseErrorBody(text), truncated: true }
      }
    }
  } catch (error: unknown) {
    if (isAbortError(error)) return { kind: 'aborted', cause: error }
    // A failed error-body read cannot mask the HTTP status that triggered it.
    return { kind: 'parsed', message: '', truncated: false }
  } finally {
    reader.releaseLock()
  }
  return { kind: 'parsed', ...parseErrorBody(text), truncated: false }
}

/** True for a fetch/`AbortSignal` abort, surfaced as cancellation by the seam. */
function isAbortError(error: unknown): boolean {
  return error instanceof DOMException && error.name === 'AbortError'
}

/** Longest raw-body fragment quoted when the body is not JSON (HTML challenges, gateway plain text). */
const MAX_FRAGMENT_CHARS = 200

/**
 * Extract the human-readable message and structural type from one error body
 * text. Honors the envelope shapes real gateways emit: OpenAI's
 * `{"error":{...}}` wrapper, Anthropic-style top-level `{type,message}`,
 * SearXNG/Exa-style `error`/`message` string fields, and Perplexity's string
 * `error`. Non-JSON bodies contribute their first line, capped, so an HTML
 * challenge page costs one diagnostic line rather than its full markup.
 * @param text - the raw body text, possibly empty.
 * @returns parsed facts with an empty message for an empty body.
 */
export function parseErrorBody(text: string): ParsedErrorBody {
  if (text.length === 0) return { message: '' }
  let body: unknown
  try {
    body = JSON.parse(text)
  } catch {
    return { message: firstLine(text) }
  }
  if (typeof body !== 'object' || body === null) {
    return { message: firstLine(text) }
  }
  const record = body as Record<string, unknown>
  const nested = typeof record.error === 'object' && record.error !== null
    ? record.error as Record<string, unknown>
    : undefined
  const message = stringField(nested?.message)
    ?? stringField(record.message)
    ?? stringField(record.detail)
    ?? stringField(record.error)
    ?? ''
  const providerType = stringField(nested?.type) ?? stringField(record.type)
  return {
    message,
    ...providerType === undefined ? {} : { providerType },
  }
}

/** First non-blank line of a raw body, capped, for non-JSON diagnostics. */
function firstLine(text: string): string {
  const line = text.split('\n', 1)[0] ?? ''
  const trimmed = line.trim()
  return trimmed.length > MAX_FRAGMENT_CHARS ? trimmed.slice(0, MAX_FRAGMENT_CHARS) : trimmed
}

/**
 * Throw the standard provider-failure {@link WebError} for one non-2xx
 * response, composing the status-line default with any richer body detail.
 * @param provider - display name of the failing provider ("DeepSeek", "Exa").
 * @param response - the non-2xx response.
 * @param parsed - facts parsed by {@link readErrorBody}.
 * @returns never; throws so callers can `throw await failWith(...)`.
 * @throws {WebError} code `WEB_PROVIDER_ERROR`, carrying `status` and
 *   `providerType` when known.
 */
export function throwProviderHttpError(
  provider: string,
  response: Response,
  parsed: ParsedErrorBody,
): never {
  const base = `${provider} API error (HTTP ${response.status})`
  const message = parsed.message.length > 0 ? parsed.message : base
  throw new WebError(message, 'WEB_PROVIDER_ERROR', {
    status: response.status,
    ...(parsed.providerType === undefined ? {} : { providerType: parsed.providerType }),
  })
}

/** Read one own string field without invoking accessors on hostile bodies. */
function stringField(value: unknown): string | undefined {
  return typeof value === 'string' && value.length > 0 ? value : undefined
}
