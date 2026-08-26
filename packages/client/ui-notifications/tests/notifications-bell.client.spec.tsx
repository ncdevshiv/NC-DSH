// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { useSyncExternalStore } from 'react'
import { act, cleanup, fireEvent, render, screen, within } from '@testing-library/react'
import type { RenderResult } from '@testing-library/react'
import { makeTranslate } from '@deepseek-ai/dsh-client-test-runtime'
import type { SnapshotStore } from '@deepseek-ai/dsh-client-runtime/client'
import { NotificationsBell, type NotificationsBellProps } from '../src/client/NotificationsBell.tsx'
import {
  createNotificationsStore,
  type NotificationsController,
  type NotificationsState,
  type UpdateStatusView,
} from '../src/client/store.ts'
import { zh } from '../src/client/locales.ts'
import {
  fakeWire, fail, holdStatus, installedView, notice, ok, STATUS_QUIESCENT, statusOffering,
} from './wire-fake.client.ts'

const NOW = Date.parse('2026-08-26T12:00:00.000Z')

beforeEach(() => {
  vi.useFakeTimers()
  vi.setSystemTime(NOW)
})

afterEach(() => {
  cleanup()
  vi.useRealTimers()
  vi.restoreAllMocks()
})

/** Microtask flush inside act so settled wire answers reach the bound hook. */
const settle = async (): Promise<void> => {
  await act(async () => { await Promise.resolve() })
}

interface Harness {
  controller: NotificationsController
  wire: ReturnType<typeof fakeWire>
  view: RenderResult
}

/**
 * Production-equivalent selector hook over the controller's store, bound with
 * React's own useSyncExternalStore (the ui-renderer binder pulls the external
 * shim, whose hoisted copy carries a nested React 18 on this checkout).
 */
function bindStore(store: SnapshotStore<NotificationsState>): NotificationsBellProps['useSnapshot'] {
  return function useSnapshot<S>(select: (state: NotificationsState) => S): S {
    return useSyncExternalStore(
      onChange => store.subscribe(onChange),
      () => select(store.getSnapshot()),
    )
  }
}

async function setup(
  options: {
    wire?: ReturnType<typeof fakeWire>
    state?: Partial<NotificationsState>
    wide?: boolean
  } = {},
): Promise<Harness> {
  const wire = options.wire ?? fakeWire()
  const controller = createNotificationsStore(wire)
  const props = {
    wide: options.wide ?? true,
    useSnapshot: bindStore(controller.store),
    controller,
    t: makeTranslate(zh),
  } as unknown as NotificationsBellProps
  let view!: RenderResult
  await act(async () => {
    view = render(<NotificationsBell {...props} />)
    if (options.state !== undefined) {
      controller.store.update((state) => { Object.assign(state, options.state) })
    }
  })
  return { controller, wire, view }
}

/** Re-seed snapshot state and flush the bound hook's re-render. */
async function seed(h: Harness, state: Partial<NotificationsState>): Promise<void> {
  await act(async () => {
    h.controller.store.update((current) => { Object.assign(current, state) })
  })
}

function trigger(): HTMLElement {
  return screen.getByRole('button', { name: zh['trigger.label'] })
}

async function openPanel(): Promise<void> {
  await act(async () => { fireEvent.click(trigger()) })
  await settle()
  // Touch the panel so a silently missing surface fails here with context.
  screen.getByRole('region', { name: zh['panel.aria'] })
}

describe('trigger badge and dot', () => {
  it('counts notices that are neither read nor dismissed', async () => {
    const h = await setup({
      state: {
        notices: [
          notice({ id: 'a' }), notice({ id: 'b' }),
          notice({ id: 'r', read: true }), notice({ id: 'd', dismissed: true }),
        ],
      },
    })
    expect(within(trigger()).getByText('2')).toBeDefined()
    expect(h.view.container.querySelector('[data-update-dot]')).toBeNull()
  })

  it('hides the badge when every notice is read or dismissed', async () => {
    await setup({ state: { notices: [notice({ id: 'd', dismissed: true }), notice({ id: 'r', read: true })] } })
    expect(within(trigger()).queryByText('1')).toBeNull()
  })

  it('shows the update dot only when a version is offered and nothing is unread', async () => {
    const offered = await setup({ wire: fakeWire({ status: ok(statusOffering()) }) })
    expect(offered.view.container.querySelector('[data-update-dot]')).not.toBeNull()

    // An unread notice takes over the attention slot from the dot.
    await seed(offered, { notices: [notice({ id: 'u' })] })
    expect(offered.view.container.querySelector('[data-update-dot]')).toBeNull()

    const ignoredCase = await setup({
      state: { updates: { ...statusOffering(), ignoredLatest: true } },
      wire: (() => {
        const gated = fakeWire()
        gated.status.mockImplementation(() => holdStatus())
        return gated
      })(),
    })
    expect(ignoredCase.view.container.querySelector('[data-update-dot]')).toBeNull()
  })

  it('renders the rail variant without the text label', async () => {
    await setup({ wide: false })
    expect(within(trigger()).queryByText(zh['trigger.label'])).toBeNull()
    expect(trigger().getAttribute('aria-label')).toBe(zh['trigger.label'])
  })
})

describe('open and close', () => {
  it('opens the panel with both sections and closes on Escape with focus returned', async () => {
    const h = await setup()
    await openPanel()
    expect(screen.getByText(zh['notices.title'])).toBeDefined()
    expect(screen.getByText(zh['sdk.title'])).toBeDefined()

    await act(async () => { fireEvent.keyDown(h.view.container.firstElementChild!, { key: 'Escape' }) })
    expect(screen.queryByRole('region', { name: zh['panel.aria'] })).toBeNull()
    expect(document.activeElement).toBe(trigger())
  })

  it('closes on an outside pointer press', async () => {
    await setup()
    await openPanel()
    await act(async () => { fireEvent.pointerDown(document.body) })
    expect(screen.queryByRole('region', { name: zh['panel.aria'] })).toBeNull()
  })

  it('toggles closed on a second trigger press and ignores unrelated keys', async () => {
    const h = await setup()
    await openPanel()
    // A non-Escape key while open changes nothing about the surface.
    await act(async () => { fireEvent.keyDown(h.view.container.firstElementChild!, { key: 'ArrowDown' }) })
    expect(screen.getByRole('region', { name: zh['panel.aria'] })).toBeDefined()

    await act(async () => { fireEvent.click(trigger()) })
    expect(screen.queryByRole('region', { name: zh['panel.aria'] })).toBeNull()

    // Escape while closed stays inert.
    await act(async () => { fireEvent.keyDown(h.view.container.firstElementChild!, { key: 'Escape' }) })
    expect(trigger().getAttribute('aria-expanded')).toBe('false')
  })

  it('pulls the notice list each time the panel opens, not while closed', async () => {
    const h = await setup()
    expect(h.wire.list).not.toHaveBeenCalled()
    await openPanel()
    expect(h.wire.list).toHaveBeenCalledTimes(1)
    await act(async () => { fireEvent.pointerDown(document.body) })
    await openPanel()
    expect(h.wire.list).toHaveBeenCalledTimes(2)
  })

  it('shows the empty-state copy when no notice survives the dismissal filter', async () => {
    await setup({
      wire: fakeWire({ list: ok({ items: [notice({ id: 'd', dismissed: true })] }) }),
    })
    await openPanel()
    expect(screen.getByText(zh['notices.empty'])).toBeDefined()
  })
})

describe('notice rows', () => {
  it('renders title, relative age, and the body snippet; dismissed rows stay hidden', async () => {
    await setup({
      wire: fakeWire({
        list: ok({ items: [
          notice({ id: 'm', title: '维护窗口', createdAt: new Date(NOW - 30 * 60_000).toISOString(), body: '例行维护说明文本' }),
          notice({ id: 'gone', dismissed: true }),
        ] }),
      }),
    })
    await openPanel()
    const rows = screen.getAllByRole('listitem')
    expect(rows).toHaveLength(1)
    expect(rows[0]!.textContent).toContain('维护窗口')
    expect(rows[0]!.textContent).toContain('30 分钟前')
    expect(rows[0]!.textContent).toContain('例行维护说明文本')
  })

  it('walks minutes, hours, days, then falls back to the date itself', async () => {
    await setup({
      wire: fakeWire({
        list: ok({ items: [
          notice({ id: 'now', createdAt: new Date(NOW - 5_000).toISOString() }),
          notice({ id: 'min', createdAt: new Date(NOW - 5 * 60_000).toISOString() }),
          notice({ id: 'hour', createdAt: new Date(NOW - 3 * 3_600_000).toISOString() }),
          notice({ id: 'day', createdAt: new Date(NOW - 4 * 86_400_000).toISOString() }),
          notice({ id: 'old', createdAt: '2026-07-10T08:00:00.000Z' }),
        ] }),
      }),
    })
    await openPanel()
    const ages = screen.getAllByRole('listitem').map((row) => {
      const main = row.firstElementChild as HTMLElement
      return main.firstElementChild!.lastElementChild!.textContent
    })
    expect(ages).toEqual(['刚刚', '5 分钟前', '3 小时前', '4 天前', '2026-07-10'])
  })

  it('renders an unparsable creation stamp as nothing rather than a wrong figure', async () => {
    await setup({
      wire: fakeWire({
        list: ok({ items: [notice({ id: 'bad', createdAt: 'not-a-timestamp' })] }),
      }),
    })
    await openPanel()
    const row = screen.getAllByRole('listitem')[0]!
    expect(row.firstElementChild!.textContent).not.toContain('前')
  })

  it('omits the body snippet for a notice published without or with an empty body', async () => {
    await setup({
      wire: fakeWire({
        list: ok({ items: [notice({ id: 'bare' }), notice({ id: 'blank', body: '' })] }),
      }),
    })
    await openPanel()
    for (const row of screen.getAllByRole('listitem')) {
      // The row button holds exactly the header pair when no snippet renders.
      expect((row.firstElementChild as HTMLElement).children).toHaveLength(1)
    }
  })

  it('marks an unread row read on click and refetches, but skips an already-read row', async () => {
    const h = await setup({
      wire: fakeWire({
        list: ok({ items: [
          notice({ id: 'u1', title: '未读通知' }),
          notice({ id: 'r1', title: '已读通知', read: true }),
        ] }),
      }),
    })
    await openPanel()

    await act(async () => { fireEvent.click(screen.getByText('未读通知')) })
    await settle()
    expect(h.wire.setRead).toHaveBeenCalledWith({ id: 'u1', read: true })
    // The post-mutation pull replaced the click-time list.
    expect(h.wire.list).toHaveBeenCalledTimes(2)

    await act(async () => { fireEvent.click(screen.getByText('已读通知')) })
    await settle()
    expect(h.wire.setRead).toHaveBeenCalledTimes(1)
  })

  it('dismisses from the ✕ button and refetches the list after settle', async () => {
    const survivors = [notice({ id: 'keep', title: '留下的' })]
    const doomed = [notice({ id: 'gone', title: '要消失的' })]
    const wire = fakeWire()
    let listCall = 0
    wire.list.mockImplementation(() => {
      listCall += 1
      return Promise.resolve(ok({ items: listCall === 1 ? doomed : survivors }))
    })
    await setup({ wire })
    await openPanel()

    const row = screen.getAllByRole('listitem')[0]!
    await act(async () => {
      fireEvent.click(within(row).getByRole('button', { name: zh['notice.dismiss'] }))
    })
    await settle()
    expect(wire.dismiss).toHaveBeenCalledWith({ id: 'gone' })
    // The refetched list answers with only the surviving row.
    expect(screen.getAllByRole('listitem')).toHaveLength(1)
    expect(screen.getByText('留下的')).toBeDefined()
  })

  it('reports a failed pull on the notices error line without dropping the panel', async () => {
    await setup({ wire: fakeWire({ list: fail('list refused') }) })
    await openPanel()
    expect(screen.getByRole('alert').textContent).toBe('list refused')
    expect(screen.getByText(zh['notices.empty'])).toBeDefined()
  })
})

describe('AI SDK card', () => {
  it('shows the installed and latest tags once a status arrived, with only check to press', async () => {
    await setup({ wire: fakeWire({ status: ok(STATUS_QUIESCENT) }) })
    await openPanel()
    // Installed v1.2.3 and latest v1.2.3 render the same string twice.
    expect(screen.getAllByText('v1.2.3')).toHaveLength(2)
    expect(screen.queryByText(zh['sdk.installed.none'])).toBeNull()
    expect(screen.getByRole('button', { name: zh['sdk.check'] })).toBeDefined()
    expect(screen.queryByRole('button', { name: zh['sdk.skip'] })).toBeNull()
  })

  it('renders placeholders until the first status answer settles', async () => {
    const wire = fakeWire()
    wire.status.mockImplementation(() => holdStatus())
    await setup({ wire })
    await openPanel()
    expect(screen.getByText(zh['sdk.installed.none'])).toBeDefined()
    expect(screen.getByText(zh['sdk.latest.none'])).toBeDefined()
    expect(screen.queryByRole('button', { name: zh['sdk.skip'] })).toBeNull()
  })

  it('offers install and skip while a non-ignored newer version exists', async () => {
    await setup({ wire: fakeWire({ status: ok(statusOffering('v2.0.0')) }) })
    await openPanel()
    expect(screen.getByRole('button', { name: '安装 v2.0.0' })).toBeDefined()
    expect(screen.getByRole('button', { name: zh['sdk.skip'] })).toBeDefined()
  })

  it('runs install through the busy labels into the success copy and notes link', async () => {
    let release: (() => void) | undefined
    // First answer (mount): an offer is up. Every later read: settled at v2.
    const settledAtV2: UpdateStatusView = {
      installed: installedView({ tag: 'v2.0.0' }),
      latest: { tag: 'v2.0.0', url: 'https://example.test/releases/v2.0.0' },
      updateAvailable: false,
      ignoredLatest: false,
    }
    const wire = fakeWire()
    let statusCall = 0
    wire.status.mockImplementation(() => {
      statusCall += 1
      return Promise.resolve(ok(statusCall === 1 ? statusOffering('v2.0.0') : settledAtV2))
    })
    wire.install.mockImplementation(async () => {
      await new Promise<void>((resolve) => { release = resolve })
      return ok({ installed: installedView({ tag: 'v2.0.0' }), restartRequired: true as const })
    })
    await setup({ wire })
    await openPanel()

    await act(async () => { fireEvent.click(screen.getByRole('button', { name: '安装 v2.0.0' })) })
    expect(wire.install).toHaveBeenCalledWith({ tag: 'v2.0.0' })
    expect(screen.getByText(zh['sdk.installing'])).toBeDefined()
    expect(screen.getByRole('button', { name: zh['sdk.skip'] }).hasAttribute('disabled')).toBe(true)
    expect(screen.getByRole('button', { name: zh['sdk.check'] }).hasAttribute('disabled')).toBe(true)

    release?.()
    await settle()
    expect(screen.getByText('已安装 v2.0.0，下次启动生效')).toBeDefined()
    const link = screen.getByRole('link', { name: zh['sdk.releaseNotes'] })
    expect(link.getAttribute('href')).toBe('https://example.test/releases/v2.0.0')
    expect(link.getAttribute('target')).toBe('_blank')
    expect(link.getAttribute('rel')).toContain('noopener')
    // The offer retired; only check remains alongside the success copy.
    expect(screen.queryByRole('button', { name: zh['sdk.skip'] })).toBeNull()
  })

  it('reports an install failure on the inline error line without the success copy', async () => {
    const wire = fakeWire({
      status: ok(statusOffering('v2.0.0')),
      install: fail('download failed'),
    })
    await setup({ wire })
    await openPanel()
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: '安装 v2.0.0' }))
    })
    await settle()
    expect(screen.getByRole('alert').textContent).toBe('download failed')
    expect(screen.queryByText(/下次启动生效/)).toBeNull()
    // A refused write changed no server fact, so no re-read ran either.
    expect(wire.status).toHaveBeenCalledTimes(1)
  })

  it('skips the version and loses the offer once the refreshed status answers', async () => {
    const ignored: UpdateStatusView = { ...statusOffering('v2.0.0'), ignoredLatest: true }
    const wire = fakeWire()
    let statusCall = 0
    wire.status.mockImplementation(() => {
      statusCall += 1
      return Promise.resolve(ok(statusCall === 1 ? statusOffering('v2.0.0') : ignored))
    })
    await setup({ wire })
    await openPanel()
    await act(async () => { fireEvent.click(screen.getByRole('button', { name: zh['sdk.skip'] })) })
    await settle()
    expect(wire.ignore).toHaveBeenCalledWith({ tag: 'v2.0.0' })
    // The trailing re-read answers "ignored", so the offer is gone.
    expect(screen.queryByRole('button', { name: '安装 v2.0.0' })).toBeNull()
  })

  it('checks now through the busy label and applies the fresh answer', async () => {
    let release: (() => void) | undefined
    const gate = new Promise<void>((resolve) => { release = resolve })
    const wire = fakeWire()
    wire.check.mockImplementation(async () => {
      await gate
      return ok(statusOffering('v9.9.9'))
    })
    await setup({ wire })
    await openPanel()

    await act(async () => { fireEvent.click(screen.getByRole('button', { name: zh['sdk.check'] })) })
    expect(screen.getByText(zh['sdk.checking'])).toBeDefined()

    release?.()
    await settle()
    expect(screen.getByRole('button', { name: '安装 v9.9.9' })).toBeDefined()
  })

  it('surfaces a rejected envelope and a host-reported lastError on the inline line', async () => {
    const refused = await setup({ wire: fakeWire({ status: fail('status refused') }) })
    await openPanel()
    expect(screen.getByRole('alert').textContent).toBe('status refused')
    refused.view.unmount()

    await setup({
      state: { updates: { ...STATUS_QUIESCENT, lastError: 'earlier check failed' } },
      wire: (() => {
        const gated = fakeWire()
        gated.status.mockImplementation(() => holdStatus())
        return gated
      })(),
    })
    await openPanel()
    expect(screen.getByRole('alert').textContent).toBe('earlier check failed')
  })

  it('shows the success copy without a notes link while no latest release is known', async () => {
    // The poll stays in flight, so the seeded snapshot is the only fact.
    const gated = fakeWire()
    gated.status.mockImplementation(() => holdStatus())
    await setup({
      wire: gated,
      state: { installedNow: 'v7.7.7', updates: null },
    })
    await openPanel()
    expect(screen.getByText('已安装 v7.7.7，下次启动生效')).toBeDefined()
    expect(screen.queryByRole('link')).toBeNull()
  })
})

describe('status freshness', () => {
  it('polls every sixty seconds and refetches when the window regains focus', async () => {
    const h = await setup()
    expect(h.wire.status).toHaveBeenCalledTimes(1)

    await act(async () => { vi.advanceTimersByTime(60_000) })
    expect(h.wire.status).toHaveBeenCalledTimes(2)

    await act(async () => { window.dispatchEvent(new Event('focus')) })
    expect(h.wire.status).toHaveBeenCalledTimes(3)

    h.view.unmount()
    await act(async () => { vi.advanceTimersByTime(120_000) })
    expect(h.wire.status).toHaveBeenCalledTimes(3)
  })
})
