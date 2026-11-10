/** Row-level settings writes: toggle ops, ordering ops, and the rail's ordering and status rules. */
import { describe, expect, it } from 'vitest'
import type { SettingsNamespaceView } from '@deepseek-ai/dsh-api-remotes/client'
import type { JsonValue } from '@deepseek-ai/dsh-util-values'
import { settingsSchema } from './settings-schema.client.ts'
import {
  disabledModelOverrideOps, modelEnabledOps, modelOrderOps, moveItem, providerEnabledOps, providerOrderOps,
} from '../src/client/mutations.ts'
import { fetchProbe } from '../src/client/row-controls.ts'
import type { ProviderRow } from '../src/client/store.ts'
import { orderRows, providerRowStatus } from '../src/client/store.ts'

describe('row-level settings operations', () => {
  it('pauses a provider by storing the refusal and unpauses it by removing it', () => {
    expect(providerEnabledOps(['providers', 'openai'], false)).toEqual([
      { op: 'set', path: ['providers', 'openai', 'enabled'], value: false },
    ])
    // Re-enabling unsets the key rather than storing a default, so a row that
    // carries nothing else stays removable.
    expect(providerEnabledOps(['providers', 'openai'], true)).toEqual([
      { op: 'unset', path: ['providers', 'openai', 'enabled'] },
    ])
    expect(providerEnabledOps([], false)).toEqual([{ op: 'set', path: ['enabled'], value: false }])
  })

  it('addresses a models entry by its position in the list', () => {
    expect(modelEnabledOps(['providers', 'acme', 'models'], 2, false)).toEqual([
      { op: 'set', path: ['providers', 'acme', 'models', '2', 'enabled'], value: false },
    ])
    expect(modelEnabledOps(['providers', 'acme', 'models'], 0, true)).toEqual([
      { op: 'unset', path: ['providers', 'acme', 'models', '0', 'enabled'] },
    ])
  })

  it('records a withdrawn catalog model as an override when the route lists no models', () => {
    // A catalog route has no `models` entry to address; the adapter merges
    // `modelOverrides` by id instead, which is how one catalog model leaves the
    // served list while the rest stay untouched.
    expect(modelEnabledOps(undefined, 3, false)).toEqual([])
    expect(disabledModelOverrideOps(['providers', 'openai', 'modelOverrides'], ['gpt-a', 'gpt-b'])).toEqual([
      { op: 'set', path: ['providers', 'openai', 'modelOverrides', 'gpt-a', 'enabled'], value: false },
      { op: 'set', path: ['providers', 'openai', 'modelOverrides', 'gpt-b', 'enabled'], value: false },
    ])
    expect(disabledModelOverrideOps(['providers', 'openai', 'modelOverrides'], [])).toEqual([])
  })

  it('clears a stored order instead of storing an empty one', () => {
    expect(modelOrderOps(['providers', 'acme'], ['m1', 'm2'])).toEqual([
      { op: 'set', path: ['providers', 'acme', 'modelOrder'], value: ['m1', 'm2'] },
    ])
    expect(modelOrderOps(['providers', 'acme'], [])).toEqual([
      { op: 'unset', path: ['providers', 'acme', 'modelOrder'] },
    ])
    expect(providerOrderOps(['acme', 'openai'])).toEqual([
      { op: 'set', path: ['providerOrder'], value: ['acme', 'openai'] },
    ])
    expect(providerOrderOps([])).toEqual([{ op: 'unset', path: ['providerOrder'] }])
  })

  it('moves one item and leaves the rest in order', () => {
    expect(moveItem(['a', 'b', 'c', 'd'], 0, 2)).toEqual(['b', 'c', 'a', 'd'])
    expect(moveItem(['a', 'b', 'c'], 2, 0)).toEqual(['c', 'a', 'b'])
    expect(moveItem(['a', 'b'], 1, 1)).toEqual(['a', 'b'])
    // Out-of-range drops are the no-op a stray keyboard event must produce.
    expect(moveItem(['a', 'b'], 5, 0)).toEqual(['a', 'b'])
    expect(moveItem(['a', 'b'], 0, -1)).toEqual(['a', 'b'])
    expect(moveItem([], 0, 0)).toEqual([])
  })
})

/** One joined provider row with the defaults a configured, enabled route carries. */
function row(provider: string, ns: string, overrides: Partial<ProviderRow> = {}): ProviderRow {
  return {
    entry: { provider, displayName: provider, settingsNs: ns, settingsPath: ['providers', provider], active: true },
    configured: true,
    enabled: true,
    removable: true,
    apiKeyEnv: undefined,
    credential: undefined,
    ...overrides,
  }
}

/** A namespace view whose resolved value is one `llm-pi-ai` section. */
function view(ns: string, providerOrder?: (string | number | null)[]): SettingsNamespaceView {
  return {
    ns,
    autoGenerate: false,
    schema: null,
    value: providerOrder === undefined ? {} : { providerOrder },
    applies: 'live',
    secrets: [],
    revision: 1,
  }
}

/** A `llm-pi-ai` view whose `relay` profile carries `profile`. */
function piAiView(profile: Record<string, JsonValue>): SettingsNamespaceView {
  return { ...view('llm-pi-ai'), value: { providers: { relay: profile } } }
}

describe('row fetch probe', () => {
  const schema = settingsSchema

  it('asks a shipped route by its id alone, letting the adapter answer from its catalog', () => {
    expect(fetchProbe(schema, row('openai', 'llm-pi-ai'), piAiView({}))).toEqual({ kind: 'ask', provider: 'openai' })
  })

  it('carries the endpoint and protocol the profile names', () => {
    expect(fetchProbe(schema, row('relay', 'llm-pi-ai'), piAiView({
      baseURL: 'https://relay.example/v1', api: 'openai-responses',
    }))).toEqual({
      kind: 'ask', provider: 'relay', baseURL: 'https://relay.example/v1', api: 'openai-responses',
    })
  })

  it('blocks a declared route that names no endpoint of its own', () => {
    const declared = row('relay', 'llm-pi-ai', {
      entry: {
        provider: 'relay', displayName: 'relay', settingsNs: 'llm-pi-ai',
        settingsPath: ['providers', 'relay'], active: true, declared: true,
      },
    })
    expect(fetchProbe(schema, declared, piAiView({}))).toEqual({ kind: 'blocked', reason: 'fetchNeedsBaseUrl' })
  })

  it('offers nothing to a family that registers no discovery, or with no namespace resolved', () => {
    expect(fetchProbe(schema, row('deepseek-official', 'llm-deepseek'), piAiView({}))).toBeUndefined()
    expect(fetchProbe(schema, row('openai', 'llm-pi-ai'), undefined)).toBeUndefined()
  })
})

describe('rail ordering', () => {
  const schema = settingsSchema

  it('leaves rows in directory order when no order is stored', () => {
    const rows = [row('deepseek-official', 'llm-deepseek'), row('openai', 'llm-pi-ai'), row('acme', 'llm-pi-ai')]
    expect(orderRows(rows, new Map(), schema).map(item => item.entry.provider))
      .toEqual(['deepseek-official', 'openai', 'acme'])
    expect(orderRows(rows, new Map([['llm-pi-ai', view('llm-pi-ai')]]), schema).map(item => item.entry.provider))
      .toEqual(['deepseek-official', 'openai', 'acme'])
  })

  it('orders named pi-ai routes and keeps the rest behind them in directory order', () => {
    const rows = [
      row('deepseek-account', ''),
      row('deepseek-official', 'llm-deepseek'),
      row('openai', 'llm-pi-ai'),
      row('acme', 'llm-pi-ai'),
      row('zeta', 'llm-pi-ai'),
    ]
    const namespaces = new Map([['llm-pi-ai', view('llm-pi-ai', ['zeta', 'acme'])]])

    // Account and official stay pinned, `zeta` and `acme` move to the front of
    // the sortable group, and the never-listed `openai` keeps its place last.
    expect(orderRows(rows, namespaces, schema).map(item => item.entry.provider))
      .toEqual(['deepseek-account', 'deepseek-official', 'zeta', 'acme', 'openai'])
  })

  it('ignores a stored entry that is not a route key', () => {
    const rows = [row('openai', 'llm-pi-ai'), row('acme', 'llm-pi-ai')]
    const namespaces = new Map([['llm-pi-ai', view('llm-pi-ai', ['acme', 7, null])]])

    expect(orderRows(rows, namespaces, schema).map(item => item.entry.provider)).toEqual(['acme', 'openai'])
  })
})

describe('row status', () => {
  const key = { configured: true, writable: true, value: undefined }

  it('ranks a configuration failure above a pause', () => {
    const failing = row('openai', 'llm-pi-ai', {
      entry: { ...row('openai', 'llm-pi-ai').entry, active: true, error: 'catalog unavailable' },
      enabled: false,
    })
    expect(providerRowStatus(failing)).toBe('error')
  })

  it('calls a paused route paused whatever its credential state', () => {
    expect(providerRowStatus(row('openai', 'llm-pi-ai', { enabled: false }))).toBe('paused')
  })

  it('calls an active route with a stored key ready and one without it attention', () => {
    expect(providerRowStatus(row('openai', 'llm-pi-ai', { apiKeyEnv: 'OPENAI_API_KEY', credential: key }))).toBe('ready')
    expect(providerRowStatus(row('openai', 'llm-pi-ai', { apiKeyEnv: 'OPENAI_API_KEY' }))).toBe('attention')
    // A keyless native-auth route needs no stored credential to be ready.
    expect(providerRowStatus(row('amazon-bedrock', 'llm-pi-ai'))).toBe('ready')
    // An unregistered route cannot serve, however complete its profile looks.
    expect(providerRowStatus(row('openai', 'llm-pi-ai', {
      entry: { ...row('openai', 'llm-pi-ai').entry, active: false },
    }))).toBe('attention')
  })

  it('leaves a catalog route the user never added neutral rather than warning', () => {
    // The rail lists dozens of dormant providers; a warning on each would read
    // as dozens of faults. Only a route somebody added, missing what it needs,
    // asks for attention.
    const dormant = row('moonshotai', 'llm-pi-ai', {
      configured: false,
      entry: { ...row('moonshotai', 'llm-pi-ai').entry, active: false },
    })
    expect(providerRowStatus(dormant)).toBe('unadopted')
    // Adding it turns the same missing key into a real gap.
    expect(providerRowStatus({ ...dormant, configured: true })).toBe('attention')
  })

  it('reads the account row through its catalog availability', () => {
    const account = row('deepseek-account', '', { accountAvailable: true })
    expect(providerRowStatus(account)).toBe('ready')
    expect(providerRowStatus(row('deepseek-account', ''))).toBe('attention')
  })
})
