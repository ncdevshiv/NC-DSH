/**
 * `@deepseek-ai/dsh-web-fetch-moli`: registers a moli-backed `WebFetchProvider`
 * with `ctx.web`. A function/namespace plugin (NOT a default-export service):
 * it registers INTO the seam's fetch registry. The provider stays unavailable
 * until the configured moli binary resolves (`$MOLI_BINARY` or `PATH`), so
 * mounting it changes nothing for deployments without the binary.
 *
 * @module @deepseek-ai/dsh-web-fetch-moli
 */

import type { Context } from '@deepseek-ai/cordis'
import z from '@deepseek-ai/schemastery'
import type {} from '@deepseek-ai/dsh-web'
import { launchEnvironmentOf } from '@deepseek-ai/dsh-launch-environment'
import { MoliFetchProvider } from './provider.ts'

const MAX_NODE_TIMER_DELAY_MS = 2_147_483_647

export {
  MOLI_FETCH_PROVIDER_ID,
  MoliFetchProvider,
  defaultMoliProber,
  validateFetchUrl,
} from './provider.ts'
export type { MoliBinaryProbe, MoliBinaryProber, MoliFetchProviderOptions } from './types.ts'

/** Cordis plugin name used by loader diagnostics. */
export const name = 'web-fetch-moli'

/** The web seam this provider registers into. */
export const inject = ['web']

/** Plugin config (all optional — `apply` fills env-var and constant defaults). */
export interface Config {
  /** The moli binary. Falls back to `$MOLI_BINARY`, then `'moli'` on PATH. */
  binaryPath?: string
  /** Maximum accepted request URL length in characters. */
  maxUrlLength?: number
  /** Maximum decoded markdown length in characters. */
  maxBodyChars?: number
  /** Resource-backstop fetch budget in milliseconds, within Node's timer range. */
  timeoutMs?: number
  /** Budget for the one-time `--version` availability probe in milliseconds. */
  probeTimeoutMs?: number
}

export const Config: z<Config> = z.object({
  binaryPath: z.string().default(''),
  maxUrlLength: z.number().default(2_048),
  maxBodyChars: z.number().default(100_000),
  timeoutMs: z.number().default(30_000),
  probeTimeoutMs: z.number().default(5_000),
})

/** Complete config after schemastery applies every field default. */
type ResolvedConfig = Required<Config>

/** A resource limit (char/timeout cap) must be a positive finite number. */
function assertPositiveFinite(name: string, value: number): void {
  if (!Number.isFinite(value) || value <= 0) {
    throw new Error(`web-fetch-moli: ${name} must be a positive finite number`)
  }
}

/** Node coerces larger timer delays to 1 ms, so reject them at configuration time. */
function assertTimeoutMs(value: number): void {
  assertPositiveFinite('timeoutMs', value)
  if (value > MAX_NODE_TIMER_DELAY_MS) {
    throw new Error(`web-fetch-moli: timeoutMs must be no greater than ${MAX_NODE_TIMER_DELAY_MS}`)
  }
}

/** Register the moli fetch provider with `ctx.web`. */
export function apply(ctx: Context, config: Config): void {
  // schemastery (Config) has already filled every defaulted field.
  const resolved = config as ResolvedConfig
  assertPositiveFinite('maxUrlLength', resolved.maxUrlLength)
  assertPositiveFinite('maxBodyChars', resolved.maxBodyChars)
  assertTimeoutMs(resolved.timeoutMs)
  assertPositiveFinite('probeTimeoutMs', resolved.probeTimeoutMs)
  const binaryPath = resolved.binaryPath.length > 0
    ? resolved.binaryPath
    : launchEnvironmentOf(ctx).get('MOLI_BINARY')?.value ?? 'moli'
  ctx.web.registerFetchProvider(new MoliFetchProvider({
    binaryPath,
    maxUrlLength: resolved.maxUrlLength,
    maxBodyChars: resolved.maxBodyChars,
    timeoutMs: resolved.timeoutMs,
    probeTimeoutMs: resolved.probeTimeoutMs,
  }))
}
