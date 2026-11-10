/** What the NC-AG Settings section registers, when, and that it leaves with the fiber. */

import { Context } from '@deepseek-ai/cordis'
import { describe, expect, it, vi } from 'vitest'
import { resolveSlotLabel } from '@deepseek-ai/dsh-client-ui-slots'
import { SlotRegistry } from '@deepseek-ai/dsh-client-ui-renderer/client'
import { RemoteError, TestRemote } from '@deepseek-ai/dsh-client-test-runtime'
import { LocaleRuntime } from '@deepseek-ai/dsh-client-locale/client'
import { apply as settingsApply, inject as settingsInject } from '@deepseek-ai/dsh-client-ui-settings/client'
import { apply, inject, NCAG_NS, NS } from '../src/client/index.ts'
import type { NcagSectionInjected } from '../src/client/index.ts'
import { apply as hostApply } from '../src/index.ts'

/** One Host view of a served namespace. */
function view(ns: string, revision = 0) {
  return { ns, schema: {}, value: {}, applies: 'live', secrets: [], revision }
}

/** The empty platform snapshot the scripted namespace answers. */
const SNAPSHOT = {
  generatedAt: '2026-11-09T00:00:00.000Z',
  projects: [],
  usage: { inputTokens: 0, outputTokens: 0, costUsd: 0, calls: 0 },
  events: [],
  audit: { records: 0, brokenAt: null },
  routes: [],
  profiles: 0,
  liveAgents: 0,
  pendingDecisions: 0,
}

/** @param served - namespaces the Host describes; omitted answers a failed read. */
async function bench(served?: string[], withPlatform = true) {
  const ctx = new Context()
  await ctx.plugin(SlotRegistry).await()
  const locale = new LocaleRuntime(ctx)
  locale.setLocale('zh')
  ctx.provide('locale', locale)
  const describeSettings = vi.fn(() => Promise.resolve(served === undefined
    ? { ok: false, error: new RemoteError('gateway/internal', 'no provider', {}) }
    : { ok: true, value: { writable: true, hasDocument: true, namespaces: served.map(ns => view(ns)) } }))
  const ncag = {
    snapshot: vi.fn(async () => ({ ok: true, value: SNAPSHOT })),
    profiles: vi.fn(async () => ({ ok: true, value: [] })),
    decisions: vi.fn(async () => ({ ok: true, value: [] })),
  }
  new TestRemote(ctx, {
    settings: { describe: describeSettings },
    ...withPlatform ? { ncag } : {},
  })
  await ctx.plugin({ inject: [...settingsInject], apply: settingsApply }).await()
  return { ctx, slots: ctx.get('slots') as SlotRegistry, ncag, describeSettings }
}

/** The settings shell's section slot, as its owner declares it. */
function declareRoot(slots: SlotRegistry): () => void {
  return slots.register({
    name: 'root',
    children: { 'settings.section': { kind: 'list', scope: 'root' } },
  } as never, () => null)
}

describe('ui-settings-ncag apply', () => {
  it('keeps the host Loader entry inert', () => {
    expect(hostApply).not.toThrow()
  })

  it('declares the services it uses', () => {
    expect(inject).toEqual(['slots', 'locale', 'configForms', 'remote'])
  })

  it('registers one settings section below Agent presets', async () => {
    const { ctx, slots } = await bench([NCAG_NS])
    declareRoot(slots)

    await ctx.plugin({ inject: [...inject], apply }).await()

    await vi.waitFor(() => { expect(slots.entries('settings.section')).toHaveLength(1) })
    const entry = slots.entries('settings.section')[0]!
    expect(entry.options).toMatchObject({ id: 'ncag', order: 25 })
    expect(resolveSlotLabel(entry.options.label)).toBe('NC-AG')
    expect(entry.locale).toBe(NS)
    const face = (entry.inject as unknown as () => NcagSectionInjected)()
    expect(Object.keys(face.hooks).sort()).toEqual(['ncagCard', 'ncagMonitor'])
  })

  it('reads the platform snapshot once the page starts', async () => {
    const { ctx, slots, ncag } = await bench([NCAG_NS])
    declareRoot(slots)
    await ctx.plugin({ inject: [...inject], apply }).await()
    await vi.waitFor(() => { expect(slots.entries('settings.section')).toHaveLength(1) })
    const face = (slots.entries('settings.section')[0]!.inject as unknown as () => NcagSectionInjected)()
    face.start()
    await vi.waitFor(() => { expect(ncag.snapshot).toHaveBeenCalled() })
    expect(face.hooks.ncagMonitor.getSnapshot().snapshot).toEqual(SNAPSHOT)
  })

  it('reports the platform as unavailable when no namespace is mounted', async () => {
    const { ctx, slots } = await bench([NCAG_NS], false)
    declareRoot(slots)
    await ctx.plugin({ inject: [...inject], apply }).await()
    await vi.waitFor(() => { expect(slots.entries('settings.section')).toHaveLength(1) })
    const face = (slots.entries('settings.section')[0]!.inject as unknown as () => NcagSectionInjected)()
    face.start()
    await vi.waitFor(() => { expect(face.hooks.ncagMonitor.getSnapshot().status).toBe('unavailable') })
  })

  it('collapses the section on teardown', async () => {
    const { ctx, slots } = await bench([NCAG_NS])
    declareRoot(slots)
    const fiber = ctx.plugin({ inject: [...inject], apply })
    await fiber.await()
    await vi.waitFor(() => { expect(slots.entries('settings.section')).toHaveLength(1) })

    await fiber.dispose()

    expect(slots.entries('settings.section')).toHaveLength(0)
  })
})
