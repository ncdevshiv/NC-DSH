/**
 * ui-notifications plugin halves: the browser entry's dictionary and
 * sidebar-footer registrations against the real SlotRegistry (with fiber
 * teardown proving removal — HMR safety), the inert node entry, and the
 * invariant companion's ownership reservation.
 */
import { Context } from '@deepseek-ai/cordis'
import { describe, expect, it } from 'vitest'
import InvariantRegistry from '@deepseek-ai/dsh-invariants'
import { SlotRegistry } from '@deepseek-ai/dsh-client-runtime/client'
import { stubSettingsScope } from '@deepseek-ai/dsh-client-test-runtime'
import { apply as applyLocale, inject as localeInject } from '@deepseek-ai/dsh-client-locale/client'
import { apply, inject } from '../src/client/index.ts'
import { apply as applyNode } from '../src/index.ts'
import * as NotificationsInvariant from '../src/invariant.ts'
import { en, NS, zh } from '../src/client/locales.ts'

/** Slot ledger reader: entry ids currently registered in the footer list. */
function footerEntryIds(ctx: Context): (string | undefined)[] {
  return ctx.slots
    .entries('sidebar.footer.action')
    .map(entry => entry.options.id)
}

/** Boot the browser half over a real slot tree; `declareFooter` controls whether the footer list exists yet. */
async function bench(declareFooter = true): Promise<{ ctx: Context; fiber: ReturnType<Context['plugin']> }> {
  const ctx = new Context()
  await ctx.plugin(SlotRegistry).await()
  if (declareFooter) {
    ctx.slots.register({
      name: 'root',
      children: {
        'sidebar.footer.action': { kind: 'list', scope: 'root' },
      },
    } as never, () => null)
  }
  // The notifications controller reads only the connection's api slice; no
  // call happens until a mounted panel drives it.
  ctx.provide('connection', { api: {} } as never)
  // The locale plugin binds a settings scope, which reads the connection handle
  // and the forwarded-event port.
  ctx.provide('remote', { $on: () => () => {} } as never)
  ctx.provide('settingsScope', { bind: () => stubSettingsScope().scope } as never)
  await ctx.plugin({ inject: localeInject, apply: applyLocale }).await()
  // These specs assert the shipped Chinese copy. There is no jsdom `window` in
  // this lane, so browser-language detection never runs and the locale comes
  // from FALLBACK_LOCALE (en): state the asserted locale explicitly.
  ctx.locale.setLocale('zh')
  const fiber = ctx.plugin({ inject: [...inject], apply })
  await fiber.await()
  return { ctx, fiber }
}

describe('ui-notifications browser half', () => {
  it('declares the services it binds', () => {
    expect(inject).toEqual(['slots', 'locale', 'connection'])
  })

  it('registers the footer bell, and fiber teardown removes it (HMR safety)', async () => {
    const { ctx, fiber } = await bench()
    expect(footerEntryIds(ctx)).toContain('notifications-bell')
    await fiber.dispose()
    expect(footerEntryIds(ctx)).not.toContain('notifications-bell')
  })

  it('waits for a late declaration, registers on its arrival, and leaves when it collapses', async () => {
    const b = await bench(false)
    expect(b.ctx.slots.entries('sidebar.footer.action')).toHaveLength(0)
    const disposeDeclaration = b.ctx.slots.register({
      name: 'root',
      children: {
        'sidebar.footer.action': { kind: 'list', scope: 'root' },
      },
    } as never, () => null)
    await Promise.resolve()
    expect(footerEntryIds(b.ctx)).toContain('notifications-bell')

    // Declarer unload cascades our entry away; the local disposer goes stale,
    // which must not block a re-declaration from re-admitting the entry.
    disposeDeclaration()
    expect(footerEntryIds(b.ctx)).not.toContain('notifications-bell')
    b.ctx.slots.register({
      name: 'root',
      children: {
        'sidebar.footer.action': { kind: 'list', scope: 'root' },
      },
    } as never, () => null)
    await Promise.resolve()
    expect(b.ctx.slots.entries('sidebar.footer.action')).toHaveLength(1)
  })

  it('binds the shared store handle and the namespace translator into the entry face', async () => {
    const b = await bench()
    const entry = b.ctx.slots.entries('sidebar.footer.action')[0]!
    expect(entry.locale).toBe(NS)
    const injected = (entry.inject as unknown as () => import('../src/client/NotificationsBell.tsx').NotificationsInjected)()
    expect(injected.hooks.snapshot.getSnapshot()).toMatchObject({
      updates: null,
      notices: [],
      installing: false,
      checking: false,
      installedNow: null,
      sdkError: null,
      noticesError: null,
    })
    expect(typeof injected.controller.install).toBe('function')
    expect(typeof injected.controller.refreshNotices).toBe('function')
  })

  it('registers both dictionaries under its own namespace and releases them with the fiber', async () => {
    const { ctx, fiber } = await bench()
    const translate = ctx.locale.bind(NS)
    expect(translate('panel.aria')).toBe(zh['panel.aria'])
    ctx.locale.setLocale('en')
    expect(translate('panel.aria')).toBe(en['panel.aria'])

    // Withdrawn dictionaries leave the key unresolved rather than translated.
    await fiber.dispose()
    expect(translate('panel.aria')).not.toBe(en['panel.aria'])
  })

  it('keeps the English dictionary key-identical to the Chinese source of truth', () => {
    expect(Object.keys(en).sort()).toEqual(Object.keys(zh).sort())
  })
})

describe('ui-notifications node half', () => {
  it('contributes no host behavior', () => {
    // The node half exists only so the plugin appears in the Loader tree.
    expect(applyNode).not.toThrow()
  })
})

describe('ui-notifications invariant companion', () => {
  it('reserves package ownership under its declared companion name', async () => {
    const ctx = new Context()
    await ctx.plugin(InvariantRegistry, { enabled: true })
    const fiber = ctx.plugin(NotificationsInvariant)
    await fiber.await()
    expect(NotificationsInvariant.name).toBe('client-ui-notifications-invariant')
    expect(NotificationsInvariant.inject).toEqual(['invariants'])
    // Emitting an unrelated event proves the companion installed no audit.
    expect(() => { (ctx.emit as (event: string) => void)('slots/changed') }).not.toThrow()
    await fiber.dispose()
  })
})
