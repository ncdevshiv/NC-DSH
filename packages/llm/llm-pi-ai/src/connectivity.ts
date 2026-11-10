/**
 * Answering "does this model actually answer?" for the configuration surface's
 * test action.
 *
 * A saved route probed without a credential of its own goes through the harness
 * seam, so the probe takes the same adapter registration, credential
 * resolution, and deployment posture a conversation request to that route
 * would. A draft — an endpoint the user has typed but not saved, a route no
 * adapter serves yet, or a key the surface is testing before storing it — has
 * no seam entry to go through, so this module resolves the draft into a profile
 * the way `discovery.ts` builds a provider from one, and hands it to the same
 * {@link PiAiAdapter} that serves real traffic. The choice is made from the
 * request and never defaulted: a one-shot credential is sent to the endpoint it
 * was typed for, because resolving the stored key instead would test a key the
 * user is not asking about.
 *
 * Nothing here simulates an answer. Every field of the result comes from one
 * real provider round trip, including the provider's own refusal text, and no
 * credential a probe supplies is stored.
 *
 * @module dsh-llm-pi-ai/connectivity
 */

import type { LlmModelTestOperation, LlmModelTestResult, LlmRuntime, StreamChunk } from '@deepseek-ai/dsh-llm'
import { errorChain, LlmError, probeMessages, settleModelProbe } from '@deepseek-ai/dsh-llm'
import { PiAiAdapter } from './adapter.ts'
import type { PiAiAuthInjection } from './adapter.ts'
import { catalogProvider } from './catalog.ts'
import type { PiAiProviderProfile, ResolvedPiAiProviderProfile } from './config.ts'
import { resolveProfiles } from './config.ts'
import { usableProbeKey } from './discovery.ts'

/** Route key for a probe that names no route: the draft is its own provider. */
const DRAFT_ROUTE = 'model-test'

/** Protocol a draft that names none is asked with, matching endpoint interrogation. */
const DRAFT_PROTOCOL = 'openai-completions'

/** The seam entry points a stored-route probe dispatches through. */
export type PiAiModelTestSeam = Pick<LlmRuntime, 'listProviders' | 'prepareCall'>

/** Host-owned route facts a draft deliberately omits. */
export interface StoredPiAiTestProfile {
  /** The configured profile, its catalog narrowed to the probed model. */
  readonly source: PiAiProviderProfile
  /**
   * Resolve the route's stored credential. Asked only when the request names
   * none, so a key typed into the form cannot fail over a stored one.
   */
  readonly resolveApiKey: () => Promise<string | undefined>
}

/** Seam and Host inputs {@link testPiAiModel} needs beyond the request. */
export interface PiAiModelTestHost {
  /** Seam entry points: route presence and one registration-bound prepared call. */
  readonly seam: PiAiModelTestSeam
  /** pi-ai auth injectables for the route a draft probe builds. */
  readonly auth: PiAiAuthInjection
  /** Stored-route facts for the route the request names, when it names one. */
  readonly storedProfile?: (provider: string, model: string) => StoredPiAiTestProfile | undefined
}

/**
 * Send one real request to one model through the pi-ai adapter family and
 * report what the provider did.
 * @param request - the draft endpoint, protocol, one-shot credential, and model
 *   to probe, with operation-local cancellation.
 * @param host - seam entry points, pi-ai auth injectables, and stored-route facts.
 * @returns the measured outcome; a refused credential, model, or request is a
 *   result carrying the provider's own failure, not a thrown error.
 */
export async function testPiAiModel(
  request: LlmModelTestOperation,
  host: PiAiModelTestHost,
): Promise<LlmModelTestResult> {
  const provider = request.provider
  // The seam path needs both halves of a stored request: a route the registry
  // serves, and no credential to try in place of the route's own.
  if (provider !== undefined
    && request.apiKey === undefined
    && host.seam.listProviders().some(info => info.id === provider)) {
    return settleModelProbe({
      model: request.model,
      ...request.signal === undefined ? {} : { signal: request.signal },
      open: signal => host.seam.prepareCall({ provider, model: request.model }, signal)
        .then(call => call.stream({ ...call.config, messages: probeMessages(), signal })),
    })
  }
  return settleModelProbe({
    model: request.model,
    ...request.signal === undefined ? {} : { signal: request.signal },
    open: signal => openDraftProbe(request, host, signal),
  })
}

/**
 * Resolve a draft into one route's profile set, refusing a route this build
 * cannot construct rather than sending a request that cannot be shaped.
 * @param provider - route key the probe dispatches under.
 * @param source - the profile the draft names, merged over the stored route's.
 * @returns the resolved single-route profile set.
 */
function draftProfiles(
  provider: string,
  source: PiAiProviderProfile,
): ReadonlyMap<string, ResolvedPiAiProviderProfile> {
  try {
    return resolveProfiles({ [provider]: source })
  } catch (error: unknown) {
    throw new LlmError(
      `llm-pi-ai: provider "${provider}" cannot serve this model test: ${errorChain(error)}`,
      'INVALID_CONFIG',
      { cause: error },
    )
  }
}

/**
 * Build the route a draft names and stream one probe request through the
 * adapter that serves real traffic for it.
 * @param request - the draft to construct from; its route key, when named,
 *   supplies the endpoint, protocol, catalog entry, and headers the draft omits.
 * @param host - pi-ai auth injectables and stored-route facts.
 * @param signal - the probe's own cancellation, already carrying the caller's.
 * @returns the chunk stream of the one probe request.
 */
async function openDraftProbe(
  request: LlmModelTestOperation,
  host: PiAiModelTestHost,
  signal: AbortSignal,
): Promise<AsyncIterable<StreamChunk>> {
  const stored = request.provider === undefined ? undefined : host.storedProfile?.(request.provider, request.model)
  const provider = request.provider ?? DRAFT_ROUTE
  const catalog = catalogProvider(provider)
  // A route the installed catalog describes brings its own endpoint and
  // protocol, so only an undeclared one needs the draft to name them, and only
  // an undeclared one takes the Chat Completions default that endpoint
  // interrogation adopts for the same reason.
  const api = request.api ?? stored?.source.api ?? (catalog === undefined ? DRAFT_PROTOCOL : undefined)
  // A form that cleared the endpoint falls back to the route's own, exactly as
  // the request path would; a draft naming no route has nothing to fall back to.
  const requested = request.baseURL === undefined || request.baseURL.length === 0 ? undefined : request.baseURL
  const baseURL = requested ?? stored?.source.baseURL
  if (baseURL === undefined && catalog === undefined) {
    throw new LlmError(
      `pi-ai ships no catalog for provider "${provider}", so this probe has no endpoint to send it to; set a baseURL`,
      'INVALID_TEST',
    )
  }
  const profiles = draftProfiles(provider, {
    ...stored?.source,
    ...api === undefined ? {} : { api },
    ...baseURL === undefined ? {} : { baseURL },
    // A named route's Host profile already carries the probed model's own
    // catalog entry; a draft declares the id alone.
    models: stored?.source.models ?? [{ id: request.model }],
  })
  const adapter = new PiAiAdapter({
    profiles: () => profiles,
    auth: host.auth,
    resolveApiKey: async () => {
      // A key typed into the form wins: it may be the replacement the user is
      // testing, and the stored credential it supersedes is never resolved.
      const supplied = request.apiKey ?? await stored?.resolveApiKey()
      return supplied === undefined ? undefined : usableProbeKey(supplied)
    },
  })
  return adapter.stream({ provider, model: request.model, messages: probeMessages(), signal })
}
