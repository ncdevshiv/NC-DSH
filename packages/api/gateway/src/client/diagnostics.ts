/**
 * Browser span reporter for the Host diagnostics rings: every Remote call's
 * round-trip is batched here and posted to `/api/diagnostics/client`, so the
 * click-to-response timeline of the Web or Desktop surface is readable from the
 * same endpoint as the Host's model and tool spans.
 *
 * Delivery is best-effort and never on the call path: a failed post is dropped,
 * the queue is bounded, and nothing here throws or logs.
 */

/** One batch cap; the Host accepts the same order of magnitude. */
const BATCH_SIZE = 25

/** Flush delay after the first queued span. */
const FLUSH_MS = 2000

/** Hard queue cap; the oldest spans are dropped once a flush keeps failing. */
const QUEUE_LIMIT = 200

/** Route relative to the document, so both the Web origin and the Desktop scheme resolve it. */
const REPORT_PATH = 'api/diagnostics/client'

/** One span awaiting delivery. */
interface PendingSpan {
  at: string
  kind: string
  name: string
  durationMs: number
  ok?: boolean
  detail?: string | undefined
}

let queue: PendingSpan[] = []
let flushTimer: ReturnType<typeof setTimeout> | undefined

/**
 * Queue one browser span for the next batch.
 * @param span - the measured span; `kind` groups it (`rpc`, `ui`, `network`).
 */
export function reportClientSpan(span: Omit<PendingSpan, 'at'>): void {
  if (typeof window === 'undefined' || typeof fetch !== 'function') return
  queue.push({ at: new Date().toISOString(), ...span })
  if (queue.length > QUEUE_LIMIT) queue = queue.slice(queue.length - QUEUE_LIMIT)
  if (queue.length >= BATCH_SIZE) flushClientSpans()
  else flushTimer ??= setTimeout(flushClientSpans, FLUSH_MS)
}

/**
 * Send the queued batch now. Safe to call at any time; failures drop the batch.
 */
export function flushClientSpans(): void {
  if (flushTimer !== undefined) {
    clearTimeout(flushTimer)
    flushTimer = undefined
  }
  if (queue.length === 0) return
  const body = JSON.stringify(queue.splice(0, BATCH_SIZE))
  try {
    void fetch(new URL(REPORT_PATH, document.baseURI), {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body,
      keepalive: true,
    }).catch(() => {})
  } catch {
    // A document without a usable base URL simply reports nothing.
  }
}

/**
 * Time one Remote round-trip and queue its span.
 * @param name - endpoint identity, for example `session/list`.
 * @param run - the call to measure.
 * @returns the call's own settlement.
 */
export async function timedRemoteCall<T>(name: string, run: () => Promise<T>): Promise<T> {
  const start = performance.now()
  try {
    const value = await run()
    reportClientSpan({ kind: 'rpc', name, durationMs: performance.now() - start, ok: true })
    return value
  } catch (error) {
    const message = error instanceof Error ? error.message.split('\n')[0]?.slice(0, 300) : String(error).slice(0, 300)
    reportClientSpan({
      kind: 'rpc',
      name,
      durationMs: performance.now() - start,
      ok: false,
      detail: message,
    })
    throw error
  }
}
