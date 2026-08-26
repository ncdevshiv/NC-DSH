/**
 * Harness error base with a stable machine-routable code and chained cause.
 * Package errors extend it so tool results and replay can retain failure class.
 * @module @deepseek-ai/dsh-llm/error
 */

/**
 * Base class for all harness errors. Carries a `code` (stable, programmatic —
 * e.g. `NO_ADAPTER`, `INVALID_ARGS`, `INVARIANT`) distinct from the
 * human-readable `message`, and supports `cause` chaining via the standard
 * `ErrorOptions`. `name` defaults to the subclass constructor name.
 */
export class HarnessError extends Error {
  /** Stable machine-routable failure class (e.g. `RATE_LIMIT`); route on this, never by parsing `message`. */
  readonly code: string

  constructor(message: string, code: string, options?: ErrorOptions) {
    super(message, options)
    this.code = code
    this.name = new.target.name
  }
}

/** Canonical provider-neutral code for a model request rejected because its context window was exceeded. */
export const CONTEXT_WINDOW_EXCEEDED_CODE = 'CONTEXT_WINDOW_EXCEEDED'

/** Canonical provider-neutral code for an exhausted account quota or balance. */
export const QUOTA_EXCEEDED_CODE = 'QUOTA'

/**
 * Canonical provider-neutral code for a response that completed normally but
 * carried no content blocks at all. Providers occasionally emit a degenerate
 * completion (a terminal stop with zero output); adapters classify it as this
 * failure instead of yielding an empty assistant message, because an empty
 * message silently ends the turn with nothing for the user or the loop to act
 * on. The attempt produced nothing durable, so retry policy treats it as safe
 * to repeat.
 */
export const EMPTY_RESPONSE_CODE = 'EMPTY_RESPONSE'

/**
 * Canonical provider-neutral code for a credential that was supplied but
 * cannot be used — malformed rather than absent. Distinct from
 * `MISSING_CREDENTIAL` because the fix differs: correct the stored value
 * rather than supply one. Deliberately outside the default retryable set —
 * a malformed credential fails identically on every attempt.
 */
export const INVALID_CREDENTIAL_CODE = 'INVALID_CREDENTIAL'

/** Canonical provider-neutral code for a request aborted by the caller. */
export const ABORTED_CODE = 'ABORTED'

/**
 * Canonical provider-neutral code for a request the provider rejected because
 * the supplied input was malformed or the model cannot handle it.
 */
export const INVALID_REQUEST_CODE = 'INVALID_REQUEST'

/**
 * Canonical provider-neutral code for a response closed by the provider's
 * transport before a terminal frame arrived. Network blips, gateway timeouts,
 * and dropped sockets all share this code.
 */
export const TRANSPORT_CODE = 'TRANSPORT'

/**
 * Canonical provider-neutral code for a transient provider failure. The retry
 * policy's default retryable set includes this code; it is not the
 * provider's own `5xx` body shape, only the harness-side classification.
 */
export const SERVER_CODE = 'SERVER'

/**
 * Canonical provider-neutral code for a request throttled by the provider.
 * Distinct from {@link QUOTA_EXCEEDED_CODE} which is terminal.
 */
export const RATE_LIMIT_CODE = 'RATE_LIMIT'

/**
 * Canonical provider-neutral code for an authentication or authorization
 * failure. Both 401 and 403 collapse to this single taxonomy entry because
 * the fix (correct the stored credential) is identical for both.
 */
export const AUTH_CODE = 'AUTH'

/** Canonical provider-neutral code for a request timed out at the harness layer. */
export const TIMEOUT_CODE = 'TIMEOUT'

/**
 * Provider-neutral code for an HTTP status the harness has no specialized
 * mapping for. Carries the numeric status through the code so a future log
 * query can group these without re-parsing the message.
 * @param status - the upstream HTTP status code.
 * @returns the stable failure code, e.g. `HTTP_429`.
 */
export function httpStatusCode(status: number): string {
  return `HTTP_${String(status)}`
}

/**
 * Structured detail the harness classifiers share: a joined string of every
 * provider-issued fact and an optional provider-specific structural type
 * (Anthropic's `api_error` / `overloaded_error` / OpenAI's `server_error`).
 * @param detail - whitespace-joined `code`/`type`/`message` text from the
 *   provider error body, when available.
 * @param providerType - provider's own structural error type field, when
 *   captured separately. Takes precedence over `detail` for context-window
 *   and quota classification, where the SDK's own `type` is the more
 *   stable signal than the wording of the message.
 */
export interface ProviderErrorDetail {
  /** Whitespace-joined provider-issued facts; empty when nothing usable. */
  readonly detail: string
  /** Provider's structural error type, when the adapter captured it. */
  readonly providerType?: string
}

/**
 * Classify one HTTP non-2xx response into a stable harness code.
 *
 * The classifier is the seam's single source of truth: every adapter maps
 * a 401 to `AUTH`, a 429 with a quota-exhausted body to `QUOTA`, a 429
 * without one to `RATE_LIMIT`, a 400 that names a context bound to
 * `CONTEXT_WINDOW_EXCEEDED`, and so on. Today the DeepSeek adapter has its
 * own copy and pi-ai has a regex soup against a flattened string; both
 * will be replaced with this helper so a 500 in the message text can no
 * longer be misclassified as `TIMEOUT`.
 * @param status - HTTP status of a non-2xx provider response.
 * @param detail - structured provider detail; empty string is allowed.
 * @returns the harness `LlmFailure.code`.
 */
export function classifyHttpStatus(status: number, detail: ProviderErrorDetail = { detail: '' }): string {
  if (!Number.isInteger(status) || status < 100 || status > 599) {
    return httpStatusCode(status)
  }
  if (status === 401 || status === 403) return AUTH_CODE
  if (status === 408) return TIMEOUT_CODE
  if (status === 413) return INVALID_REQUEST_CODE
  if (status === 400) {
    if (isContextWindowExceededError(detail.detail)) return CONTEXT_WINDOW_EXCEEDED_CODE
    return INVALID_REQUEST_CODE
  }
  if (status === 429) {
    if (isQuotaExceededError(detail.detail)) return QUOTA_EXCEEDED_CODE
    return RATE_LIMIT_CODE
  }
  if (status >= 500 && status <= 599) return SERVER_CODE
  return httpStatusCode(status)
}

/** Structured codes and plain phrases that explicitly name a context bound being exceeded. */
const STRUCTURED_CONTEXT_OVERFLOW = new RegExp(
  String.raw`(?:^|[^a-z0-9])context[\s_-](?:length|window)[\s_-]`
  + String.raw`(?:exceed(?:ed|s)?|overflow(?:ed)?|limit[\s_-]exceeded)(?:$|[^a-z0-9])`,
  'i',
)

/** Request-size wording that ties "too large" directly to model context capacity. */
const TOO_LARGE_FOR_CONTEXT = new RegExp(
  String.raw`\b(?:request|prompt|input|messages?)\s+(?:is\s+|are\s+)?`
  + String.raw`too\s+(?:large|long)\s+for\s+(?:(?:this|the)\s+)?`
  + String.raw`(?:model(?:'s)?\s+)?context(?:\s+window)?\b`,
  'i',
)

/** "Exceeds" wording is safe only when its object is explicitly the model context. */
const EXCEEDS_MODEL_CONTEXT = new RegExp(
  String.raw`\b(?:input|prompt|request|messages?)\b.{0,40}`
  + String.raw`\b(?:exceed(?:s|ed)?|overflows?|is\s+larger\s+than)\b.{0,40}`
  + String.raw`\b(?:the\s+)?(?:model(?:'s)?\s+)?context(?:\s+(?:length|window))?\b`,
  'i',
)

/**
 * Recognize the context-overflow wording used by OpenAI-compatible providers
 * and library adapters. Adapters pass all available provider code, type, and
 * message text so both thrown and in-band delivery styles share one classifier.
 * @param detail - provider error code/type/message text joined into one string.
 * @returns true when the detail identifies a request exceeding the model context window.
 */
export function isContextWindowExceededError(detail: string): boolean {
  return STRUCTURED_CONTEXT_OVERFLOW.test(detail)
    || /\b(?:maximum|max)(?:\s+(?:allowed|supported))?\s+context\s+(?:length|window)\b/i.test(detail)
    || TOO_LARGE_FOR_CONTEXT.test(detail)
    || /\b(?:input|prompt|request)\s+(?:is\s+)?too\s+(?:long|large)\s+for\s+(?:this|the)\s+model\b/i.test(detail)
    || EXCEEDS_MODEL_CONTEXT.test(detail)
}

/**
 * Recognize provider wording that identifies an exhausted account quota rather
 * than a transient request-rate limit.
 * @param detail - provider error code/type/message text joined into one string.
 * @returns true only for terminal quota, balance, credit, budget, or usage-limit wording.
 */
export function isQuotaExceededError(detail: string): boolean {
  return /\binsufficient[\s_-]+(?:quota|balance|credits?)\b/i.test(detail)
    || /\b(?:quota|usage[\s_-]+limit)[\s_-]+(?:exceeded|exhausted|reached)\b/i.test(detail)
    || /\bexceed(?:ed|s)?[\s_-]+(?:(?:your|the)[\s_-]+)?(?:current[\s_-]+)?quota\b/i.test(detail)
    || /\b(?:balance|credits?)[\s_-]+(?:exhausted|depleted)\b/i.test(detail)
    || /\bout[\s_-]+of[\s_-]+(?:credits?|budget)\b/i.test(detail)
}

/**
 * Render a thrown value with its full `cause` chain and AggregateError
 * members, so transport wrappers like undici's `TypeError: fetch failed`
 * surface the underlying failure instead of masking it. Plain structured
 * failures render their own data-backed `message`. Diagnostic-surface
 * rendering only (messages, notices, logs) — never parse the result; route on
 * {@link HarnessError.code}.
 * @param value - the caught value (`unknown` in catch clauses).
 * @returns the outermost message first, each cause appended with `: ` (skipped
 * when it repeats the wrapper message verbatim), and AggregateError members
 * bracketed and `; `-joined.
 */
export function errorChain(value: unknown): string {
  // Tracks the active recursion path (entries removed on exit), so only true
  // cycles are flagged and a diamond-shared cause still renders in full.
  const path = new Set<unknown>()
  const render = (current: unknown): string => {
    if (path.has(current)) return '<circular cause>'
    path.add(current)
    try {
      if (!(current instanceof Error)) {
        if (typeof current === 'object' && current !== null) {
          const descriptor = Object.getOwnPropertyDescriptor(current, 'message')
          if (descriptor !== undefined && 'value' in descriptor && typeof descriptor.value === 'string') {
            return descriptor.value
          }
        }
        return String(current)
      }
      const message = current.message === '' ? current.name : current.message
      const members = current instanceof AggregateError && current.errors.length > 0
        ? ` [${current.errors.map(render).join('; ')}]`
        : ''
      const causeText = current.cause === undefined || current.cause === null
        ? ''
        : render(current.cause)
      // Wrappers like `new HarnessError(String(value), code, { cause: value })`
      // repeat their cause verbatim; rendering it again would only add noise.
      const cause = causeText === '' || causeText === message ? '' : `: ${causeText}`
      return `${message}${members}${cause}`
    } catch {
      // Only hostile coercion or hostile accessors (a throwing toString /
      // Symbol.toPrimitive on a non-Error, or a throwing message/name/cause/
      // errors getter on an Error subclass): this renderer feeds UI notices
      // and logs, so nothing may escape. Inner frames catch their own throws,
      // so only the hostile node collapses, not the whole chain.
      return '<unrenderable value>'
    } finally {
      path.delete(current)
    }
  }
  return render(value)
}

/**
 * Narrow an arbitrary thrown value to a HarnessError (for `instanceof` at runtime boundaries).
 * @param value - the caught value (`unknown` in catch clauses).
 * @returns true only for real instances; duck-typed or cross-realm errors do not narrow.
 */
export function isHarnessError(value: unknown): value is HarnessError {
  return value instanceof HarnessError
}
