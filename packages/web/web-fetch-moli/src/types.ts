/**
 * Resolved option and injectable-boundary types for the moli fetch provider.
 * Types only — no runtime code.
 * @module @deepseek-ai/dsh-web-fetch-moli/types
 */

import type { NativeCommandRunner } from '@deepseek-ai/dsh-native-command'

/**
 * The result shape of the one-time binary probe behind `available()`. Mirrors
 * the `spawnSync` fields the provider inspects; an injectable prober returns
 * this so tests never spawn a real process.
 */
export interface MoliBinaryProbe {
  /** Process exit code, or `null` when the process failed to spawn. */
  readonly status: number | null
  /** Spawn failure (e.g. `ENOENT`), or `null` when the process ran. */
  readonly error: unknown
}

/** A synchronous one-shot probe of the moli binary. */
export type MoliBinaryProber = (binaryPath: string, timeoutMs: number) => MoliBinaryProbe

/** Complete provider options after the plugin resolves every default. */
export interface MoliFetchProviderOptions {
  /** The moli binary: a PATH name or an absolute/relative executable path. */
  binaryPath: string
  /** Maximum accepted request URL length in characters. */
  maxUrlLength: number
  /** Character cap on returned markdown; a longer body is truncated and flagged. */
  maxBodyChars: number
  /**
   * Resource-backstop fetch budget in milliseconds, composed with the caller's
   * signal — not the model-facing tool-call budget (`dsh-tool-call-timeout-policy`).
   */
  timeoutMs: number
  /** Budget for the one-time `--version` availability probe in milliseconds. */
  probeTimeoutMs: number
  /** The subprocess runner; defaults to {@link runNativeCommand}-equivalent behavior. */
  runner?: NativeCommandRunner
  /** The availability prober; defaults to a real `spawnSync(['--version'])`. */
  prober?: MoliBinaryProber
}
