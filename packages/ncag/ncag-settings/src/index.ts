/**
 * Host-owned NC-AG settings. The Web settings tab edits this namespace, and the
 * platform samples it on every dispatch, so a saved change applies to the next
 * project or operation without restarting the process.
 *
 * @module @deepseek-ai/dsh-ncag-settings
 */
import type { Volatile } from '@deepseek-ai/cordis'
import { Context, Service } from '@deepseek-ai/cordis'
import z from '@deepseek-ai/schemastery'
import type {} from '@deepseek-ai/dsh-settings'
import { createModelPrices } from '@deepseek-ai/dsh-ncag-orchestrator'
import type { NcagRuntimeSettings } from '@deepseek-ai/dsh-ncag-orchestrator'
import type { ModelRoute, ModelTier } from '@deepseek-ai/dsh-ncag-core'

declare module '@deepseek-ai/cordis' {
  interface Context {
    /** NC-AG settings owned by the settings document, when this row is mounted. */
    ncagSettings: NcagSettings
  }
}

/** Plain stored NC-AG preferences. */
export interface NcagSettingsSnapshot {
  /** Fast-tier route override written `provider/model`; empty follows the main model. */
  readonly fastModel?: string
  /** Standard-tier route override written `provider/model`. */
  readonly standardModel?: string
  /** Reasoning-tier route override written `provider/model`. */
  readonly reasoningModel?: string
  /** JSON price overrides; empty records tokens without USD. */
  readonly pricingOverrides?: string
  /** Live-agent ceiling; 0 is unlimited. */
  readonly uiMaxConcurrentAgents?: number
  /** Helper ceiling per task; 0 is unlimited. */
  readonly uiMaxAgentsPerTask?: number
  /** Model steps per task; 0 follows the deployment default. */
  readonly uiMaxStepsPerTask?: number
  /** Output tokens per call; 0 follows the deployment default. */
  readonly uiMaxOutputTokens?: number
  /** Project spend ceiling in USD; 0 is unlimited. */
  readonly uiMaxCostUsd?: number
}

/** Stored NC-AG preferences as the settings service reads them: plain or settings-owned. */
export type NcagSettingsValues = {
  readonly [K in keyof NcagSettingsSnapshot]?: NcagSettingsSnapshot[K] | Volatile<NonNullable<NcagSettingsSnapshot[K]>>
}

/** Configuration validator. */
export const Config = z.object({
  fastModel: z.string().default('').volatile(),
  standardModel: z.string().default('').volatile(),
  reasoningModel: z.string().default('').volatile(),
  pricingOverrides: z.string().default('').volatile(),
  uiMaxConcurrentAgents: z.number().default(0).volatile(),
  uiMaxAgentsPerTask: z.number().default(0).volatile(),
  uiMaxStepsPerTask: z.number().default(0).volatile(),
  uiMaxOutputTokens: z.number().default(0).volatile(),
  uiMaxCostUsd: z.number().default(0).volatile(),
})

/** Plugin name; also the settings namespace the tab edits. */
export const name = 'ncag-settings'

/**
 * Read one setting that may be a plain value, a settings-owned reference, or absent.
 * @param value - the configured value.
 * @param fallback - value used when the field is absent.
 * @returns the current setting.
 */
function readSetting<T>(value: T | Volatile<T> | undefined, fallback: T): T {
  if (value === undefined) return fallback
  return typeof value === 'object' && value !== null && 'get' in value && typeof value.get === 'function'
    ? value.get() as T
    : value as T
}

/**
 * Parse a route override written as `provider/model`.
 * @param text - the stored value.
 * @returns the route, or undefined when the field is empty.
 * @throws Error when the text is not `provider/model`.
 */
export function routeOverride(text: string): ModelRoute | undefined {
  const trimmed = text.trim()
  if (trimmed.length === 0) return undefined
  const slash = trimmed.indexOf('/')
  if (slash <= 0 || slash === trimmed.length - 1) {
    throw new Error(`ncag: model override ${JSON.stringify(trimmed)} must be written as provider/model`)
  }
  return { provider: trimmed.slice(0, slash), model: trimmed.slice(slash + 1) }
}

/**
 * Read the live settings into the runtime view the orchestrator samples.
 * @param values - the validated settings values.
 * @returns effective overrides and limits; zero limits are unlimited.
 */
export function runtimeSettingsOf(values: NcagSettingsValues): NcagRuntimeSettings {
  const overrides: Partial<Record<ModelTier, ModelRoute>> = {}
  for (const tier of ['fast', 'standard', 'reasoning'] as const) {
    const field = tier === 'fast' ? values.fastModel : tier === 'standard' ? values.standardModel : values.reasoningModel
    const route = routeOverride(readSetting(field, ''))
    if (route !== undefined) overrides[tier] = route
  }
  const pricingText = readSetting(values.pricingOverrides, '').trim()
  const pricing = pricingText.length === 0 ? [] : createModelPrices(pricingText)
  const limit = (value: number): number | null => value > 0 ? value : null
  return {
    modelOverrides: overrides,
    pricing,
    limits: {
      maxConcurrentAgents: limit(readSetting(values.uiMaxConcurrentAgents, 0)),
      maxAgentsPerTask: limit(readSetting(values.uiMaxAgentsPerTask, 0)),
      maxStepsPerTask: limit(readSetting(values.uiMaxStepsPerTask, 0)),
      maxOutputTokens: limit(readSetting(values.uiMaxOutputTokens, 0)),
      maxCostUsd: limit(readSetting(values.uiMaxCostUsd, 0)),
    },
  }
}

/** Host settings service for NC-AG. */
export default class NcagSettings extends Service {
  /** Configuration validator. */
  static Config = Config

  /**
   * @param ctx - the owning Cordis context.
   * @param values - validated settings values.
   */
  constructor(ctx: Context, private readonly values: NcagSettingsValues) {
    super(ctx, 'ncagSettings')
    ctx.inject(['settings'], (child) => { child.effect(() => child.settings.configure({ auto: false }, ctx.fiber)) })
  }

  /**
   * Sample the current settings.
   * @returns effective overrides and limits.
   */
  runtime(): NcagRuntimeSettings {
    return runtimeSettingsOf(this.values)
  }
}
