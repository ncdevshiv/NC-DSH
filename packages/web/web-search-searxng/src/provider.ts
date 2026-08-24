/**
 * `SearxngSearchProvider`: a `WebSearchProvider` backed by a SearXNG instance's JSON search API
 * (`GET /search?q=<query>&format=json`). It maps aggregated results to citeable sources, joins
 * non-blank instant answers into `content`, and expects a private instance: public ones usually
 * disable the JSON format and answer with HTTP 403, which surfaces as `WEB_PROVIDER_ERROR`.
 * @module @deepseek-ai/dsh-web-search-searxng/provider
 */

import { WebError } from '@deepseek-ai/dsh-web'
import type {
  WebSearchProvider,
  WebSearchRequest,
  WebSearchResult,
  WebSearchSource,
} from '@deepseek-ai/dsh-web'
import type { SearxngError, SearxngResult, SearxngSearchResponse } from './types.ts'

/** Stable id this provider registers under. */
export const SEARXNG_PROVIDER_ID = 'searxng'

/** Attribution header sent on every request. Bump with the package version. */
const USER_AGENT = 'deepseek-harness/0.0.1'

/** Resolved provider options (the plugin's `apply` supplies the env-var default). */
export interface SearxngSearchProviderOptions {
  /**
   * Endpoint base of the SearXNG instance; `/search` is appended. There is no
   * default because instances are self-hosted; empty/unparseable makes the
   * provider unavailable.
   */
  baseURL: string
  /** Basic-auth username for instances behind an authenticated reverse proxy. Empty = none. */
  username: string
  /** Basic-auth password; set together with `username` — a half-configured pair makes the provider unavailable. */
  password: string
}

/**
 * Map one SearXNG result to a normalized source, or `undefined` when it has no
 * usable URL (aggregation can emit url-less entries; the seam has no source
 * without a URL to cite).
 *
 * @param result - one entry of SearXNG's `results[]`.
 * @returns the normalized source, or `undefined` when the entry carries no non-blank URL.
 */
export function mapSearxngResult(result: SearxngResult): WebSearchSource | undefined {
  if (result.url == null || result.url.trim().length === 0) return undefined
  return {
    url: result.url,
    ...result.title != null && result.title.trim().length > 0 ? { title: result.title } : {},
    ...result.content != null && result.content.trim().length > 0 ? { snippet: result.content } : {},
    ...result.publishedDate != null && result.publishedDate.trim().length > 0 ? { publishedAt: result.publishedDate } : {},
  }
}

/**
 * Map a SearXNG response envelope to a normalized search result.
 *
 * @param response - the parsed `GET /search?format=json` response body.
 * @returns the normalized result; url-less entries are dropped
 *   ({@link mapSearxngResult}) and non-blank `answers[]` instant answers join
 *   into `content`.
 */
export function mapSearxngResponse(response: SearxngSearchResponse): WebSearchResult {
  const sources = (response.results ?? [])
    .map(mapSearxngResult)
    .filter((source): source is WebSearchSource => source !== undefined)
  const answers = (response.answers ?? []).filter(answer => answer.trim().length > 0)
  // Instant answers are the only generated text SearXNG produces; without them
  // `content` is omitted. The web service owns the final `maxResults`
  // truncation, so this provider reports `truncated: false`.
  return {
    ...answers.length > 0 ? { content: answers.join('\n\n') } : {},
    sources,
    truncated: false,
  }
}

/** The SearXNG-backed search provider; HTTP redirects fail as `WEB_PROVIDER_ERROR`. */
export class SearxngSearchProvider implements WebSearchProvider {
  readonly id = SEARXNG_PROVIDER_ID

  constructor(private readonly options: SearxngSearchProviderOptions) {}

  available(): boolean {
    return this.options.baseURL.length > 0
      && isValidBaseUrl(this.options.baseURL)
      // Basic auth is a pair: a half-configured pair cannot authenticate.
      && (this.options.username === '') === (this.options.password === '')
  }

  async search(request: WebSearchRequest, signal?: AbortSignal): Promise<WebSearchResult> {
    let response: Response
    try {
      response = await fetch(`${this.options.baseURL}/search?q=${encodeURIComponent(request.query)}&format=json`, {
        redirect: 'error',
        headers: {
          'accept': 'application/json',
          'user-agent': USER_AGENT,
          ...this.options.username !== '' && this.options.password !== ''
            ? { authorization: basicAuthHeader(this.options.username, this.options.password) }
            : {},
        },
        ...signal !== undefined ? { signal } : {},
      })
    } catch (error: unknown) {
      if (isAbortError(error)) throw new WebError('SearXNG search aborted', 'WEB_ABORTED', { cause: error })
      // A refused redirect (`redirect: 'error'`) lands here too: credentials
      // and query must not follow a redirect to another origin.
      throw new WebError(`SearXNG search request failed: ${String(error)}`, 'WEB_PROVIDER_ERROR', { cause: error })
    }

    if (!response.ok) {
      const status = response.status
      let message = `SearXNG API error (HTTP ${status})`
      try {
        const parsed = await response.json() as SearxngError
        const detail = parsed.error ?? parsed.message
        if (detail !== undefined && detail.length > 0) message = detail
      } catch (error: unknown) {
        // An abort fired mid-body must surface as WEB_ABORTED, not be swallowed
        // into a generic HTTP-error message — cancellation is not a provider
        // error (the seam's cancellation contract).
        if (isAbortError(error)) throw new WebError('SearXNG search aborted', 'WEB_ABORTED', { cause: error })
        // Otherwise: the HTTP status is already captured in `message` above; a
        // malformed/non-JSON error body (normal for gateway 5xx/429s and HTML
        // challenge pages) can only cost a richer provider message, never the real error.
      }
      throw new WebError(message, 'WEB_PROVIDER_ERROR')
    }

    try {
      const payload = await response.json() as SearxngSearchResponse
      return mapSearxngResponse(payload)
    } catch (error: unknown) {
      if (isAbortError(error)) throw new WebError('SearXNG search aborted', 'WEB_ABORTED', { cause: error })
      throw new WebError(`SearXNG returned an unprocessable response body: ${String(error)}`, 'WEB_PROVIDER_ERROR', { cause: error })
    }
  }
}

/** Build the HTTP basic-auth header value. */
function basicAuthHeader(username: string, password: string): string {
  return `Basic ${btoa(`${username}:${password}`)}`
}

/** True when `baseURL` parses as an absolute URL (a cheap local config check). */
function isValidBaseUrl(baseURL: string): boolean {
  return URL.canParse(baseURL)
}

/** True for a fetch/`AbortSignal` abort, surfaced as `WEB_ABORTED`. */
function isAbortError(error: unknown): boolean {
  return error instanceof DOMException && error.name === 'AbortError'
}
