/**
 * URL validation for the moli fetch provider — the pure, subprocess-free half.
 * The same transport hygiene as the local HTTP(S) provider: http(s) only, no
 * embedded credentials, bounded length.
 * @module @deepseek-ai/dsh-web-fetch-moli/policy
 */

import { WebError } from '@deepseek-ai/dsh-web'

/**
 * Validate a request URL before any subprocess starts. Returns the parsed
 * `URL`. Throws {@link WebError} otherwise. (SSRF / private-network blocking
 * is deferred at the seam level — see the package README.)
 *
 * @param input - the raw URL string from the fetch request.
 * @param maxUrlLength - the configured URL length cap in characters.
 * @returns the parsed `URL`.
 */
/* jscpd:ignore-start -- the same transport hygiene family as web-fetch-http, kept local because
   cross-package src imports are forbidden; each copy takes its cap from its own config. */
export function validateFetchUrl(input: string, maxUrlLength: number = 2_048): URL {
  if (input.length > maxUrlLength) {
    throw new WebError(`URL exceeds the maximum length of ${maxUrlLength}`, 'WEB_INVALID_URL')
  }
  let url: URL
  try {
    url = new URL(input)
  } catch (error: unknown) {
    throw new WebError(`invalid URL: ${input}`, 'WEB_INVALID_URL', { cause: error })
  }
  if (url.protocol !== 'http:' && url.protocol !== 'https:') {
    throw new WebError(`unsupported URL scheme "${url.protocol}" (only http and https are allowed)`, 'WEB_INVALID_URL')
  }
  if (url.username.length > 0 || url.password.length > 0) {
    throw new WebError('credentials in URLs are not allowed', 'WEB_BLOCKED_URL')
  }
  return url
}
/* jscpd:ignore-end */
