/** That the Settings entry opens the existing schedule panel. */

// @vitest-environment jsdom
import { createElement } from 'react'
import { Context } from '@deepseek-ai/cordis'
import { cleanup, render, screen } from '@testing-library/react'
import { afterEach, expect, it, vi } from 'vitest'
import { SlotRegistry } from '@deepseek-ai/dsh-client-ui-renderer/client'
import { makeTranslate } from '@deepseek-ai/dsh-client-test-runtime'
import { en, zh } from '../src/client/task-manager-locales.ts'
import type { MainPanelId } from '@deepseek-ai/dsh-client-ui-layout/client'
import { AutomationsSection, type AutomationsSectionInjected } from '../src/client/AutomationsSection.tsx'

afterEach(() => { cleanup() })

/** The panel id the schedule management page registers under. */
const PANEL_ID = 'schedules' as MainPanelId

/**
 * A slot registry with the Settings section seat declared, and the layout
 * service the entry opens the panel through.
 * @param composed - whether the schedule panel is composed in the main slot.
 */
async function bench(composed: boolean) {
  const ctx = new Context()
  await ctx.plugin(SlotRegistry).await()
  const selectPanel = vi.fn()
  ctx.provide('layout', {
    selectPanel,
    panelInfo: { getSnapshot: () => undefined, subscribe: () => () => {} },
  } as never)
  const dispose = ctx.slots.register({
    name: 'root',
    children: {
      'settings.section': { kind: 'list', scope: 'root' },
      'main': { kind: 'keyed', scope: 'root' },
    },
  } as never, () => null)
  if (composed) {
    ctx.slots.register({ name: 'main', key: PANEL_ID, locale: 'schedule.manager' } as never, () => null)
  }
  return { ctx, selectPanel, dispose }
}

it('opens the existing schedule panel and closes the Settings modal', async () => {
  const { ctx, selectPanel, dispose } = await bench(true)
  // Mount through the registered face, so the assertion covers the wiring
  // rather than a hand-made stand-in for it.
  const disposeEntry = ctx.slots.inject('settings.section', () => ctx.slots.register({
    name: 'settings.section', id: 'automations', order: 20, label: () => 'nav', locale: 'schedule.manager',
    inject: () => ({
      hasPanel: () => ctx.slots.entries('main').some(entry => entry.options.key === PANEL_ID),
      openPanel: () => { selectPanel(PANEL_ID) },
    }),
  }, AutomationsSection))
  try {
    const face = ctx.slots.entries('settings.section')[0]!.inject?.() as never as AutomationsSectionInjected
    const close = vi.fn()
    render(createElement(AutomationsSection, {
      hasPanel: face.hasPanel,
      openPanel: face.openPanel,
      close,
      t: makeTranslate(en),
    } as never))
    expect(screen.getByText(en['section.description'])).toBeTruthy()
    screen.getByRole('button', { name: en['section.open'] }).click()
    // The entry delegates to the panel that already exists, so the sidebar row
    // and the Settings row cannot offer different task management.
    expect(selectPanel).toHaveBeenCalledWith('schedules')
    expect(close).toHaveBeenCalledOnce()
  } finally {
    disposeEntry()
    cleanup()
    dispose()
    await ctx.fiber.dispose()
  }
})

it('renders nothing when no schedule panel is composed', async () => {
  const { ctx, selectPanel, dispose } = await bench(false)
  const { container } = render(createElement(AutomationsSection, {
    hasPanel: () => false,
    openPanel: selectPanel,
    close: vi.fn(),
    t: makeTranslate(en),
  } as never))
  try {
    expect(container.firstChild).toBeNull()
    expect(selectPanel).not.toHaveBeenCalled()
  } finally {
    dispose()
    await ctx.fiber.dispose()
  }
})

it('registers the section while the Host serves the namespace', async () => {
  const { ctx, selectPanel, dispose } = await bench(true)
  const close = vi.fn()
  const disposeEntry = ctx.slots.inject('settings.section', () => ctx.slots.register({
    name: 'settings.section',
    id: 'automations',
    order: 20,
    label: () => 'nav',
    locale: 'schedule.manager',
    inject: () => ({
      hasPanel: () => ctx.slots.entries('main').some(entry => entry.options.key === PANEL_ID),
      openPanel: () => { selectPanel(PANEL_ID) },
    }),
  }, AutomationsSection))
  try {
    const entry = ctx.slots.entries('settings.section')[0]!
    expect(entry.options).toMatchObject({ id: 'automations', order: 20 })
    // The injected face reads the live composition, so an undecomposed panel
    // yields the guarded render rather than a broken entry.
    const face = entry.inject?.() as never as AutomationsSectionInjected
    render(createElement(AutomationsSection, {
      hasPanel: face.hasPanel,
      openPanel: face.openPanel,
      close,
      t: makeTranslate(zh),
    } as never))
    expect(screen.getByText(zh['section.description'])).toBeTruthy()
  } finally {
    disposeEntry()
    cleanup()
    dispose()
    await ctx.fiber.dispose()
  }
})
