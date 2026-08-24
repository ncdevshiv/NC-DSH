/**
 * `@deepseek-ai/dsh-browser-moli`: registers a moli-backed `BrowserProvider`
 * with `ctx.browser`. A function/namespace plugin (NOT a default-export
 * service): it registers INTO the seam's provider registry. The provider stays
 * unavailable until the configured moli binary resolves (`$MOLI_BINARY` or
 * `PATH`), so mounting it changes nothing for deployments without the binary.
 *
 * @module @deepseek-ai/dsh-browser-moli
 */

import type { Context } from '@deepseek-ai/cordis'
import z from '@deepseek-ai/schemastery'
import type {} from '@deepseek-ai/dsh-browser'
import { launchEnvironmentOf } from '@deepseek-ai/dsh-launch-environment'
import { MoliBrowserProvider } from './provider.ts'

export {
  MOLI_BROWSER_PROVIDER_ID,
  MoliBrowserProvider,
  buildServeArgv,
  killServeProcess,
  reserveEphemeralPort,
} from './provider.ts'
export { CdpConnection, discoverPageTarget } from './cdp.ts'
export { MoliBrowserSession } from './session.ts'
export type { MoliBrowserProviderOptions, SpawnFn, FetchFn, WebSocketFactory } from './types.ts'

/** Cordis plugin name used by loader diagnostics. */
export const name = 'browser-moli'

/** The browser seam this provider registers into. */
export const inject = ['browser']

/** Plugin config (all optional — `apply` fills env-var and constant defaults). */
export interface Config {
  /** The moli binary. Falls back to `$MOLI_BINARY`, then `'moli'` on PATH. */
  binaryPath?: string
  /** Budget for one session's server startup in milliseconds. */
  startupTimeoutMs?: number
  /** Budget for one page navigation in milliseconds. */
  navigationTimeoutMs?: number
  /** Character cap on returned page text content. */
  maxContentChars?: number
  /** Budget for the one-time `--version` availability probe in milliseconds. */
  probeTimeoutMs?: number
  /** Interval between readiness polls in milliseconds. */
  pollEveryMs?: number
  /** Extra argv appended to the `moli serve` invocation verbatim. */
  extraServeArgs?: string[]
}

export const Config: z<Config> = z.object({
  binaryPath: z.string().default(''),
  startupTimeoutMs: z.number().default(15_000),
  navigationTimeoutMs: z.number().default(30_000),
  maxContentChars: z.number().default(100_000),
  probeTimeoutMs: z.number().default(5_000),
  pollEveryMs: z.number().default(100),
  extraServeArgs: z.array(z.string()).default([]),
})

/** Complete config after schemastery applies every field default. */
type ResolvedConfig = Required<Config>

/** A resource/timing cap must be a positive finite number. */
function assertPositiveFinite(name: string, value: number): void {
  if (!Number.isFinite(value) || value <= 0) {
    throw new Error(`browser-moli: ${name} must be a positive finite number`)
  }
}

/** Register the moli browser provider with `ctx.browser`. */
export function apply(ctx: Context, config: Config): void {
  // schemastery (Config) has already filled every defaulted field.
  const resolved = config as ResolvedConfig
  assertPositiveFinite('startupTimeoutMs', resolved.startupTimeoutMs)
  assertPositiveFinite('navigationTimeoutMs', resolved.navigationTimeoutMs)
  assertPositiveFinite('maxContentChars', resolved.maxContentChars)
  assertPositiveFinite('probeTimeoutMs', resolved.probeTimeoutMs)
  assertPositiveFinite('pollEveryMs', resolved.pollEveryMs)
  const binaryPath = resolved.binaryPath.length > 0
    ? resolved.binaryPath
    : launchEnvironmentOf(ctx).get('MOLI_BINARY')?.value ?? 'moli'
  ctx.browser.registerProvider(new MoliBrowserProvider({
    binaryPath,
    startupTimeoutMs: resolved.startupTimeoutMs,
    navigationTimeoutMs: resolved.navigationTimeoutMs,
    maxContentChars: resolved.maxContentChars,
    probeTimeoutMs: resolved.probeTimeoutMs,
    pollEveryMs: resolved.pollEveryMs,
    extraServeArgs: resolved.extraServeArgs,
  }))
}
