import { describe, expect, it } from 'vitest'
import { routeOverride, runtimeSettingsOf } from '../src/index.ts'

describe('ncag settings', () => {
  it('parses provider/model overrides and rejects malformed ones', () => {
    expect(routeOverride('')).toBeUndefined()
    expect(routeOverride(' p/m ')).toEqual({ provider: 'p', model: 'm' })
    expect(routeOverride('a/b/c')).toEqual({ provider: 'a', model: 'b/c' })
    expect(() => routeOverride('provider')).toThrow(/provider\/model/)
    expect(() => routeOverride('/model')).toThrow(/provider\/model/)
    expect(() => routeOverride('provider/')).toThrow(/provider\/model/)
  })

  it('maps zero limits to unlimited and empty prices to token-only', () => {
    const runtime = runtimeSettingsOf({
      fastModel: 'p/fast',
      standardModel: '',
      reasoningModel: 'p/deep',
      pricingOverrides: '[{"provider":"p","model":"fast","inputPerMillion":1,"outputPerMillion":2}]',
      uiMaxConcurrentAgents: 0,
      uiMaxAgentsPerTask: 2,
      uiMaxStepsPerTask: 0,
      uiMaxOutputTokens: 512,
      uiMaxCostUsd: 0,
    })
    expect(runtime.pricing).toHaveLength(1)
    expect(runtime.modelOverrides).toEqual({
      fast: { provider: 'p', model: 'fast' },
      reasoning: { provider: 'p', model: 'deep' },
    })
    expect(runtime.limits).toEqual({
      maxConcurrentAgents: null,
      maxAgentsPerTask: 2,
      maxStepsPerTask: null,
      maxOutputTokens: 512,
      maxCostUsd: null,
    })
    expect(runtimeSettingsOf({}).limits.maxCostUsd).toBeNull()
  })
})
