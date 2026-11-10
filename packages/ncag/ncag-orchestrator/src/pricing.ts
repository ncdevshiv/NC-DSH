/**
 * Parsing and validation of the deployment's model price overrides.
 *
 * Prices are optional. A route without an override still runs and records its
 * token counts; only a priced route contributes USD. The standalone profile
 * refuses an empty table at its own entry point, where cost accounting is the
 * operator's explicit choice.
 * @module @deepseek-ai/dsh-ncag-orchestrator/pricing
 */

import type { ModelPrice } from './router.ts'

/**
 * Parse the JSON price override table.
 * @param text - JSON array of price entries.
 * @returns the validated price entries; an empty array means no overrides.
 * @throws Error when the JSON is malformed or an entry is incomplete.
 */
export function createModelPrices(text: string): ModelPrice[] {
  const parsed: unknown = JSON.parse(text)
  if (!Array.isArray(parsed)) {
    throw new Error('ncag: the model price table must be a JSON array')
  }
  const prices: ModelPrice[] = []
  for (const entry of parsed) {
    if (entry === null || typeof entry !== 'object') throw new Error('ncag: every price entry must be an object')
    const record = entry as Record<string, unknown>
    const provider = record.provider
    const model = record.model
    const inputPerMillion = record.inputPerMillion
    const outputPerMillion = record.outputPerMillion
    if (typeof provider !== 'string' || provider.length === 0) throw new Error('ncag: a price entry needs a provider name')
    if (typeof model !== 'string' || model.length === 0) throw new Error('ncag: a price entry needs a model id')
    if (typeof inputPerMillion !== 'number' || !Number.isFinite(inputPerMillion) || inputPerMillion < 0) {
      throw new Error(`ncag: price entry ${provider}/${model} needs a non-negative inputPerMillion`)
    }
    if (typeof outputPerMillion !== 'number' || !Number.isFinite(outputPerMillion) || outputPerMillion < 0) {
      throw new Error(`ncag: price entry ${provider}/${model} needs a non-negative outputPerMillion`)
    }
    const tier = record.tier
    if (tier !== undefined && tier !== 'reasoning' && tier !== 'standard' && tier !== 'fast') {
      throw new Error(`ncag: price entry ${provider}/${model} has unknown tier ${JSON.stringify(tier)}`)
    }
    prices.push({
      provider,
      model,
      inputPerMillion,
      outputPerMillion,
      ...(tier === undefined ? {} : { tier: tier }),
    })
  }
  return prices
}
