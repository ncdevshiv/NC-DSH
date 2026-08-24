import { afterEach, describe, expect, it } from 'vitest'
import { Context, type Plugin } from '@deepseek-ai/cordis'
import Loader from '@deepseek-ai/cordis-plugin-loader'
import { remoteMethods } from '@deepseek-ai/dsh-typert-protocol'
import PluginInventoryGateway from '../src/index.ts'

const contexts: Context[] = []

afterEach(async () => {
  await Promise.all(contexts.splice(0).map(ctx => ctx.fiber.dispose()))
})

const activePlugin: Plugin.Function = () => {}
const pendingPlugin: Plugin.Object = {
  inject: ['neverReady'],
  apply() {},
}
// Publishes a service so a second same-module instance collides with it —
// the duplicate-registration failure the replacement path reconciles.
const duplicateProvider: Plugin.Function = (ctx) => { ctx.provide('duplicateService', true) }

async function harness(): Promise<{
  ctx: Context
  inventory: PluginInventoryGateway
}> {
  const ctx = new Context()
  contexts.push(ctx)
  await ctx.plugin(Loader)
  ctx.loader.builtins.active = activePlugin
  ctx.loader.builtins.pending = pendingPlugin
  await ctx.plugin(PluginInventoryGateway)
  const inventory = ctx.get('pluginInventory') as PluginInventoryGateway
  return { ctx, inventory }
}

describe('PluginInventoryGateway', () => {
  it('publishes direct list and setEnabled methods under the pluginInventory namespace', async () => {
    const { inventory } = await harness()
    expect(inventory.typertRemote).toMatchObject({
      serviceKey: 'pluginInventory',
      namespace: 'pluginInventory',
    })
    expect(remoteMethods(inventory)).toEqual(expect.arrayContaining([
      { method: 'list', invocation: { kind: 'direct' } },
      { method: 'setEnabled', invocation: { kind: 'direct' } },
    ]))
    expect(remoteMethods(inventory)).toHaveLength(2)
  })

  it('projects current non-group Loader entries without a second cache', async () => {
    const { ctx, inventory } = await harness()
    const activeId = await ctx.loader.create({ name: 'cordis:active' })
    const pendingId = await ctx.loader.create({ name: 'cordis:pending' })
    const disabledId = await ctx.loader.create({
      name: 'cordis:not-installed',
      disabled: true,
    })
    await ctx.loader.create({ name: 'cordis:active', group: true })

    const snapshot = inventory.list()
    expect(snapshot.entries).toHaveLength(3)
    expect(snapshot.entries).toEqual(expect.arrayContaining([
      {
        entryId: activeId,
        moduleName: 'cordis:active',
        enabled: true,
        fiberPhase: 'active',
      },
      {
        entryId: pendingId,
        moduleName: 'cordis:pending',
        enabled: true,
        fiberPhase: 'pending',
      },
      {
        entryId: disabledId,
        moduleName: 'cordis:not-installed',
        enabled: false,
        fiberPhase: null,
      },
    ]))

    await ctx.loader.update(activeId, { disabled: true })
    expect(inventory.list().entries.find(entry => entry.entryId === activeId)).toEqual({
      entryId: activeId,
      moduleName: 'cordis:active',
      enabled: false,
      fiberPhase: null,
    })

    await ctx.loader.remove(pendingId)
    expect(inventory.list().entries.some(entry => entry.entryId === pendingId)).toBe(false)
  })

  it('toggles enablement via setEnabled', async () => {
    const { ctx, inventory } = await harness()
    const id = await ctx.loader.create({ name: 'cordis:active' })
    expect(inventory.list().entries.find(e => e.entryId === id)?.enabled).toBe(true)

    const disable = await inventory.setEnabled(id as never, false)
    expect(disable.ok).toBe(true)
    expect(inventory.list().entries.find(e => e.entryId === id)?.enabled).toBe(false)
    expect(inventory.list().entries.find(e => e.entryId === id)?.fiberPhase).toBe(null)

    const enable = await inventory.setEnabled(id as never, true)
    expect(enable.ok).toBe(true)
    expect(inventory.list().entries.find(e => e.entryId === id)?.enabled).toBe(true)
    expect(inventory.list().entries.find(e => e.entryId === id)?.fiberPhase).toBe('active')

    const noop = await inventory.setEnabled(id as never, true)
    expect(noop.ok).toBe(true)
  })

  it('rejects group entries and missing ids', async () => {
    const { ctx, inventory } = await harness()
    const groupId = await ctx.loader.create({ name: 'cordis:active', group: true })
    const groupResult = await inventory.setEnabled(groupId as never, false)
    expect(groupResult.ok).toBe(false)
    expect(groupResult.message).toMatch(/Group/)

    const missing = await inventory.setEnabled('missing' as never, false)
    expect(missing.ok).toBe(false)
  })

  it('enables an entry whose module already runs by replacing the active duplicate', async () => {
    const { ctx, inventory } = await harness()
    ctx.loader.builtins['dup-provider'] = duplicateProvider
    const firstId = await ctx.loader.create({ name: 'cordis:dup-provider' })
    const secondId = await ctx.loader.create({ name: 'cordis:dup-provider', disabled: true })

    const result = await inventory.setEnabled(secondId as never, true)

    // The direct enable collided with the first instance's service; the
    // gateway displaced it and the target now owns the service alone.
    expect(result).toEqual({ ok: true })
    const rows = inventory.list().entries
    expect(rows.find(row => row.entryId === firstId)).toMatchObject({ enabled: false, fiberPhase: null })
    expect(rows.find(row => row.entryId === secondId)).toMatchObject({ enabled: true, fiberPhase: 'active' })
    expect(ctx.get('duplicateService')).toBe(true)
  })

  it('restores the displaced providers when the replacement itself fails', async () => {
    const { ctx, inventory } = await harness()
    ctx.loader.builtins['dup-provider'] = duplicateProvider
    const firstId = await ctx.loader.create({ name: 'cordis:dup-provider' })
    const secondId = await ctx.loader.create({ name: 'cordis:dup-provider', disabled: true })

    // Fail only the retry (the second enable of the target), not the initial
    // attempt whose failure triggers displacement.
    const realUpdate = ctx.loader.update.bind(ctx.loader)
    let enableAttempts = 0
    ;(ctx.loader as { update: typeof realUpdate }).update = async (id, options) => {
      if (id === secondId && (options as { disabled?: unknown } | undefined)?.disabled === null) {
        enableAttempts += 1
        if (enableAttempts > 1) throw new Error('synthetic replacement failure')
      }
      return realUpdate(id, options)
    }

    const result = await inventory.setEnabled(secondId as never, true)

    expect(result.ok).toBe(false)
    expect(result.message).toContain('synthetic replacement failure')
    expect(enableAttempts).toBe(2)
    // The previously active provider is running again; the target stays disabled.
    const rows = inventory.list().entries
    expect(rows.find(row => row.entryId === firstId)).toMatchObject({ enabled: true, fiberPhase: 'active' })
    expect(rows.find(row => row.entryId === secondId)).toMatchObject({ enabled: false, fiberPhase: null })
    expect(ctx.get('duplicateService')).toBe(true)
  })

  it('reports an unrelated enable failure verbatim without touching other entries', async () => {
    const { ctx, inventory } = await harness()
    const healthyId = await ctx.loader.create({ name: 'cordis:active' })
    const missingId = await ctx.loader.create({ name: './missing-plugin.mjs', disabled: true })

    const result = await inventory.setEnabled(missingId as never, true)

    // No active same-module entry exists, so nothing is displaced and the
    // loader's own import failure reaches the caller unchanged.
    expect(result.ok).toBe(false)
    expect(result.message).toMatch(/failed to import loader entry/)
    expect(inventory.list().entries.find(row => row.entryId === healthyId)?.fiberPhase).toBe('active')
  })
})
