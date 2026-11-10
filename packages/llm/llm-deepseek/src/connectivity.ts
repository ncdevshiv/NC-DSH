/**
 * Answering "does this model actually answer?" for the DeepSeek family's
 * configuration surface.
 *
 * A registered route probed without a credential of its own goes through the
 * harness seam, so the probe takes the route's stored authentication, retry
 * policy, and request extensions exactly as a conversation request would.
 * Everything else — a draft endpoint, a route that does not register, or a key
 * the surface is testing before storing it — is dispatched by constructing the
 * connection from the request, the way `resolveAdapterOptions` builds one from
 * a settings section. The choice is made from the request and never defaulted:
 * a one-shot credential is sent to the endpoint it was typed for rather than
 * resolved past.
 *
 * The result is measured on a live provider round trip. Nothing here simulates
 * an answer, and no credential a probe supplies is stored.
 *
 * @module dsh-llm-deepseek/connectivity
 */

import type { LlmModelTestOperation, LlmModelTestResult, LlmRuntime, StreamChunk } from '@deepseek-ai/dsh-llm'
import { INVALID_CREDENTIAL_CODE, LlmError, normalizeApiKey, probeMessages, settleModelProbe } from '@deepseek-ai/dsh-llm'
import type { AnonymousUserId } from '@deepseek-ai/dsh-anonymous-user-id'
import { DeepSeekAdapter } from './adapter.ts'
import { resolveAdapterOptions } from './config.ts'
import type { DeepSeekConnectionOptions, DeepSeekRequestAuth } from './types.ts'

/** Wire protocol this family serves: Messages content over the Messages endpoint. */
const MESSAGES_PROTOCOL = 'anthropic-messages'

/** Authentication header the Messages API reads an API key from. */
const API_KEY_HEADER = 'x-api-key'

/** Route key for a probe that names no route; the draft is its own provider. */
const DRAFT_ROUTE = 'model-test'

/** The seam entry points a stored-route probe dispatches through. */
export type DeepSeekModelTestSeam = Pick<LlmRuntime, 'listProviders' | 'prepareCall'>

/** Host inputs {@link testDeepSeekModel} needs beyond the request. */
export interface DeepSeekModelTestHost {
  /** Seam entry points: route presence and one registration-bound prepared call. */
  readonly seam: DeepSeekModelTestSeam
  /** Current connection facts of the route under test, including its endpoint. */
  readonly options: () => DeepSeekConnectionOptions
  /**
   * Authentication the named route performs from its own connection snapshot.
   * Asked only when the request names a route and supplies no credential: a
   * stored key never travels to an endpoint the route does not already serve.
   */
  readonly routeAuth: () => Promise<DeepSeekRequestAuth>
  /** Harness attribution id carried on every Messages request. */
  readonly resolveUserId: () => AnonymousUserId
}

/**
 * Send one real Messages request to one model and report what the provider did.
 * @param request - the draft endpoint, protocol, one-shot credential, and model
 *   to probe, with operation-local cancellation.
 * @param host - seam entry points and the route's own connection facts.
 * @returns the measured outcome; a refused credential, model, or request is a
 *   result carrying the provider's own failure, not a thrown error.
 */
export async function testDeepSeekModel(
  request: LlmModelTestOperation,
  host: DeepSeekModelTestHost,
): Promise<LlmModelTestResult> {
  return settleModelProbe({
    model: request.model,
    ...request.signal === undefined ? {} : { signal: request.signal },
    open: signal => openProbe(request, host, signal),
  })
}

/**
 * Dispatch the one probe request, choosing the seam or the draft path.
 * @param request - the probe request.
 * @param host - seam entry points and route facts.
 * @param signal - the probe's own cancellation, already carrying the caller's.
 * @returns the chunk stream the probe reads until it can answer.
 */
async function openProbe(
  request: LlmModelTestOperation,
  host: DeepSeekModelTestHost,
  signal: AbortSignal,
): Promise<AsyncIterable<StreamChunk>> {
  if (request.api !== undefined && request.api !== MESSAGES_PROTOCOL) {
    throw new LlmError(
      `the DeepSeek adapter serves the Messages protocol only; a probe naming "${request.api}" cannot be dispatched here`,
      'UNSUPPORTED_OPTION',
    )
  }
  const provider = request.provider
  if (provider !== undefined
    && request.apiKey === undefined
    && host.seam.listProviders().some(info => info.id === provider)) {
    const call = await host.seam.prepareCall({ provider, model: request.model }, signal)
    return call.stream({ ...call.config, messages: probeMessages(), signal })
  }
  return openDraftProbe(request, host, signal)
}

/**
 * Build the connection a draft names and stream one probe request through it.
 * @param request - the draft to construct from; its route key, when named,
 *   supplies the endpoint and catalog entry the draft omits.
 * @param host - the route's own connection facts and authentication.
 * @param signal - the probe's own cancellation, already carrying the caller's.
 * @returns the chunk stream of the one probe request.
 */
async function openDraftProbe(
  request: LlmModelTestOperation,
  host: DeepSeekModelTestHost,
  signal: AbortSignal,
): Promise<AsyncIterable<StreamChunk>> {
  const route = request.provider ?? DRAFT_ROUTE
  const stored = request.provider === undefined ? undefined : host.options()
  // A form that cleared the endpoint falls back to the route's own, exactly as
  // the request path would; a draft naming no route has nothing to fall back to.
  const requested = request.baseURL === undefined || request.baseURL.length === 0 ? undefined : request.baseURL
  const baseURL = requested ?? stored?.baseURL
  if (baseURL === undefined) {
    throw new LlmError(`llm-deepseek: provider "${route}" has no endpoint to send the probe to; set a baseURL`, 'INVALID_TEST')
  }
  const entry = stored?.models.find(model => model.id === request.model)
  const connection = resolveAdapterOptions({ baseURL, models: [entry ?? { id: request.model }] })
  // A key typed into the form is the one being tested, possibly the replacement
  // for a stored key that is failing, so it goes to this endpoint alone. A probe
  // that names a route falls back to that route's own authentication; one that
  // names only an endpoint carries no credential at all, because a stored key
  // must not travel to a URL the route does not already serve.
  const auth: DeepSeekRequestAuth = request.apiKey !== undefined
    ? { headers: { [API_KEY_HEADER]: usableProbeKey(request.apiKey) } }
    : request.provider === undefined
      ? { headers: {} }
      : await host.routeAuth()
  const adapter = new DeepSeekAdapter({
    options: () => connection,
    resolveAuth: () => Promise.resolve(auth),
    resolveUserId: host.resolveUserId,
    // A connectivity probe contributes no plugin request fields: it answers
    // whether this endpoint serves this model, not whether an extension works.
    prepareExtensions: () => Promise.resolve({ fields: {}, accept: async () => {} }),
  })
  return adapter.stream({ provider: route, model: request.model, messages: probeMessages(), signal })
}

/**
 * Accept one probe key, or refuse it before the request is built. A key no
 * HTTP header can carry would otherwise surface as an opaque transport failure
 * rather than the local, deterministic fault it is.
 * @param raw - the key typed into the form.
 * @returns the trimmed, usable key.
 */
function usableProbeKey(raw: string): string {
  const checked = normalizeApiKey(raw)
  if (checked.ok) return checked.value
  throw new LlmError(
    checked.reason === 'empty'
      ? 'this provider\'s API key is blank; enter it on the Models page, or clear it to test the stored credential'
      : 'this provider\'s API key contains characters no HTTP header can carry; paste the raw key only',
    INVALID_CREDENTIAL_CODE,
  )
}
