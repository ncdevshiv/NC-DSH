import { describe, expect, it, vi } from 'vitest'
import type { SettingsPathOpView } from '@deepseek-ai/dsh-api-remotes/client'
import { stubConfigForm, type StubConfigForm } from '@deepseek-ai/dsh-client-test-runtime'
import { NcagCardController, type NcagSettings } from '../src/client/ncag-card-controller.ts'

const DEFAULTS: NcagSettings = {
  fastModel: '', standardModel: '', reasoningModel: '', pricingOverrides: '',
  uiMaxConcurrentAgents: 0, uiMaxAgentsPerTask: 0, uiMaxStepsPerTask: 0,
  uiMaxOutputTokens: 0, uiMaxCostUsd: 0,
}

/** Make the stub behave like a Host that accepts every write. */
function acceptWrites(host: StubConfigForm<NcagSettings>): void {
  const section = (): Record<string, unknown> => ({ ...host.scope.getSnapshot().value as object })
  const layer = (): Record<string, unknown> => ({ ...host.scope.getSnapshot().user as object })
  host.mutate.mockImplementation((ops: readonly SettingsPathOpView[]) => {
    const value = { ...section() }
    const user = { ...layer() }
    for (const op of ops) {
      const field = op.path[0]!
      if (op.op === 'set') {
        value[field] = op.value
        user[field] = op.value
      } else {
        Reflect.deleteProperty(user, field)
        value[field] = (host.scope.getSnapshot().base as Record<string, unknown> | undefined)?.[field]
      }
    }
    host.publish({ value: value as unknown as NcagSettings, user })
    return Promise.resolve(true)
  })
}

describe('NcagCardController', () => {
  it('publishes stored values and stages every override and limit', async () => {
    const host = stubConfigForm<NcagSettings>()
    acceptWrites(host)
    const face = new NcagCardController(host.scope).inject()
    const state = () => face.hooks.ncagCard.getSnapshot()
    host.publish({
      status: 'ready', writable: true, revision: 4,
      value: { ...DEFAULTS, fastModel: 'p/fast', uiMaxCostUsd: 5 }, user: {},
    })

    expect(state().fastModel.text).toBe('p/fast')
    expect(state().uiMaxCostUsd.text).toBe('5')

    face.edit('standardModel', 'p/standard')
    face.edit('uiMaxAgentsPerTask', '2')
    face.save()
    await vi.waitFor(() => { expect(state().saving).toBe(false) })

    expect(host.mutate).toHaveBeenCalledWith([
      { op: 'set', path: ['standardModel'], value: 'p/standard' },
      { op: 'set', path: ['uiMaxAgentsPerTask'], value: 2 },
    ], 4)
    expect(state()).toMatchObject({ dirty: false, failed: false })
  })

  it('blocks a save for a malformed route or a negative limit', () => {
    const host = stubConfigForm<NcagSettings>()
    host.publish({ status: 'ready', writable: true, value: DEFAULTS, user: {} })
    const face = new NcagCardController(host.scope).inject()
    const state = () => face.hooks.ncagCard.getSnapshot()

    for (const draft of ['provider', '/model', 'provider/']) {
      face.edit('fastModel', draft)
      expect(state().invalid).toBe(true)
    }
    face.edit('fastModel', 'provider/model')
    expect(state().invalid).toBe(false)
    for (const draft of ['-1', '1.5', 'wat']) {
      face.edit('uiMaxConcurrentAgents', draft)
      expect(state().invalid).toBe(true)
    }
    face.edit('uiMaxConcurrentAgents', '0')
    expect(state().invalid).toBe(false)
    face.discard()
    expect(state().dirty).toBe(false)
    face.save()
    expect(host.mutate).not.toHaveBeenCalled()
  })

  it('renders unavailable while the Host serves no namespace', () => {
    const host = stubConfigForm<NcagSettings>()
    host.publish({ status: 'unavailable', writable: false })
    const face = new NcagCardController(host.scope).inject()
    expect(face.hooks.ncagCard.getSnapshot().available).toBe(false)
    face.save()
    expect(host.mutate).not.toHaveBeenCalled()
  })
})
