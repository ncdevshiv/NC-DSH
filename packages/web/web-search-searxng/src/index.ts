/**
 * `@deepseek-ai/dsh-web-search-searxng`: registers a SearXNG-backed
 * `WebSearchProvider` with `ctx.web`. A function/namespace plugin (NOT a
 * default-export service): a search provider does not own the `ctx.web` key —
 * it registers INTO the seam's provider registry, exactly as
 * `@deepseek-ai/dsh-llm-deepseek` registers an adapter into `ctx.llm`. The key
 * is owned by `@deepseek-ai/dsh-web`.
 *
 * @module @deepseek-ai/dsh-web-search-searxng
 */

import type { Context } from '@deepseek-ai/cordis'
import { launchEnvironmentOf } from '@deepseek-ai/dsh-launch-environment'
import z from '@deepseek-ai/schemastery'
import type {} from '@deepseek-ai/dsh-web'
import { SearxngSearchProvider } from './provider.ts'

export {
  SEARXNG_PROVIDER_ID,
  SearxngSearchProvider,
} from './provider.ts'
export type { SearxngSearchProviderOptions } from './provider.ts'

/** Cordis plugin name used by loader diagnostics. */
export const name = 'web-search-searxng'

/** The web seam this provider registers into. */
export const inject = ['web']

/** Plugin config (all optional — `apply` fills the env-var default). */
export interface Config {
  /** SearXNG instance base; `/search` is appended. Falls back to `$SEARXNG_BASE_URL`. Empty → provider unavailable. */
  baseURL?: string
  /** Basic-auth username for instances behind an authenticated reverse proxy. Defaults to ''. */
  username?: string
  /** Basic-auth password; set together with `username` or the provider stays unavailable. Defaults to ''. */
  password?: string
}

export const Config: z<Config> = z.object({
  baseURL: z.string(),
  username: z.string(),
  password: z.string(),
})

/** HTTP basic auth transmits `user:pass` through `btoa`, which encodes Latin-1 only. */
const LATIN1_PATTERN = /^[\u0000-\u00ff]*$/

/**
 * Fail loud when a configured credential pair cannot encode into a basic-auth
 * header. Without this check the failure surfaces per search as
 * `WEB_PROVIDER_ERROR` wrapping an `InvalidCharacterError` from `btoa`.
 */
function assertBasicAuthEncodable(username: string, password: string): void {
  if (!LATIN1_PATTERN.test(username) || !LATIN1_PATTERN.test(password)) {
    throw new Error('web-search-searxng: username and password must be Latin-1 encodable for HTTP basic auth')
  }
}

/** Register the SearXNG search provider with `ctx.web`. */
export function apply(ctx: Context, config: Config): void {
  const username = config.username ?? ''
  const password = config.password ?? ''
  if (username !== '' || password !== '') {
    assertBasicAuthEncodable(username, password)
  }
  ctx.web.registerSearchProvider(new SearxngSearchProvider({
    // Every environment layer may name this key: the product trusts the
    // project it is launched in, and the managed store is not involved here.
    baseURL: config.baseURL ?? launchEnvironmentOf(ctx).get('SEARXNG_BASE_URL')?.value ?? '',
    username,
    password,
  }))
}
