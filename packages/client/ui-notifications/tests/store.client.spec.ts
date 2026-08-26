/** Notifications store: status/notice reads, install/skip/check, and inline errors. */
import { describe, expect, it, vi } from 'vitest'
import type { UpdatesNotificationsApi } from '../src/client/store.ts'
import { createNotificationsStore, messageOf } from '../src/client/store.ts'
import { fail, installedView, notice, ok, STATUS_QUIESCENT, statusOffering } from './wire-fake.client.ts'

function flush(): Promise<void> {
  return new Promise((resolve) => { queueMicrotask(resolve) })
}

/** A face whose unstubbed methods would fail the test loudly if ever hit. */
function never(): never {
  throw new Error('unexpected wire method')
}

const wire = (methods: Record<string, unknown>): UpdatesNotificationsApi => methods as unknown as UpdatesNotificationsApi

describe('status reads', () => {
  it('applies a successful status and clears that domain error', async () => {
    const api = wire({
      status: vi.fn(() => Promise.resolve(ok(STATUS_QUIESCENT))),
      check: never, install: never, ignore: never, list: never, setRead: never, dismiss: never,
    })
    const controller = createNotificationsStore(api)
    controller.store.update((state) => { state.sdkError = 'stale' })
    await controller.refreshStatus()
    expect(controller.store.getSnapshot().updates).toEqual(STATUS_QUIESCENT)
    expect(controller.store.getSnapshot().sdkError).toBeNull()
  })

  it('surfaces a rejected envelope on the updates error line', async () => {
    const controller = createNotificationsStore(wire({ status: () => Promise.resolve(fail('status down')) }))
    await controller.refreshStatus()
    expect(controller.store.getSnapshot()).toMatchObject({ updates: null, sdkError: 'status down' })
  })

  it('stringifies a transport rejection', async () => {
    const controller = createNotificationsStore(wire({ status: () => Promise.reject(new Error('transport down')) }))
    await expect(controller.refreshStatus()).resolves.toBeUndefined()
    expect(controller.store.getSnapshot().sdkError).toBe('transport down')
  })
})

describe('checkNow', () => {
  it('drives the checking flag through the flight and applies the check answer', async () => {
    let release: (() => void) | undefined
    const gate = new Promise<void>((resolve) => { release = resolve })
    const offering = statusOffering('v2.0.0')
    const check = vi.fn(async () => {
      await gate
      return ok(offering)
    })
    const controller = createNotificationsStore(wire({ check }))
    const done = controller.checkNow()
    await flush()
    expect(controller.store.getSnapshot().checking).toBe(true)

    // A second press while the first is in flight must not re-enter the wire.
    await controller.checkNow()
    expect(check).toHaveBeenCalledTimes(1)

    release?.()
    await done
    expect(controller.store.getSnapshot()).toMatchObject({ checking: false, updates: offering })
  })
})

describe('install', () => {
  it('stages the tag, flips the busy flag through the flight, and shows the success copy', async () => {
    let release: (() => void) | undefined
    const gate = new Promise<void>((resolve) => { release = resolve })
    const install = vi.fn(async () => {
      await gate
      return ok({ installed: installedView({ tag: 'v2.0.0' }), restartRequired: true as const })
    })
    // The settled re-read answers quiescent-at-v2: nothing newer is offered,
    // so the just-staged tag stays on show.
    const status = vi.fn(() => Promise.resolve(ok({ ...STATUS_QUIESCENT, installed: installedView({ tag: 'v2.0.0' }) })))
    const controller = createNotificationsStore(wire({ install, status }))
    const done = controller.install('v2.0.0')
    await flush()
    expect(install).toHaveBeenCalledWith({ tag: 'v2.0.0' })
    expect(controller.store.getSnapshot().installing).toBe(true)

    // A second press while the first is in flight must not re-enter the wire.
    await controller.install('v3.0.0')
    expect(install).toHaveBeenCalledTimes(1)

    release?.()
    await done
    const snapshot = controller.store.getSnapshot()
    expect(snapshot.installing).toBe(false)
    expect(snapshot.installedNow).toBe('v2.0.0')
    // The success copy comes from the settled re-read, not the request alone.
    expect(status).toHaveBeenCalledTimes(1)
    expect(snapshot.updates?.updateAvailable).toBe(false)
  })

  it('reports a business rejection on the updates line without touching the success copy or re-reading', async () => {
    const status = vi.fn(() => Promise.resolve(ok(STATUS_QUIESCENT)))
    const controller = createNotificationsStore(wire({
      install: () => Promise.resolve(fail('download failed')),
      status,
    }))
    await controller.install('v2.0.0')
    expect(controller.store.getSnapshot()).toMatchObject({ installing: false, installedNow: null, sdkError: 'download failed' })
    expect(status).not.toHaveBeenCalled()
  })

  it('reopens the ordinary offer once a newer version arrives', async () => {
    // First re-read: settled at v2 with nothing offered. Later poll: a newer
    // release appears, which must retire the success copy.
    const statuses = [
      ok({ ...STATUS_QUIESCENT, installed: installedView({ tag: 'v2.0.0' }) }),
      ok(statusOffering('v3.0.0')),
    ]
    const controller = createNotificationsStore(wire({
      install: () => Promise.resolve(ok({
        installed: installedView({ tag: 'v2.0.0' }),
        restartRequired: true as const,
      })),
      status: () => Promise.resolve(statuses.shift()!),
    }))
    await controller.install('v2.0.0')
    expect(controller.store.getSnapshot().installedNow).toBe('v2.0.0')
    await controller.refreshStatus()
    expect(controller.store.getSnapshot().installedNow).toBeNull()
  })

  it('ignores a re-entry while an install is already in flight', async () => {
    const install = vi.fn(() => new Promise<{ result: { ok: false; error: { message: string } } }>(() => {}))
    const controller = createNotificationsStore(wire({ install }))
    void controller.install('v2.0.0')
    await flush()
    await controller.install('v3.0.0')
    expect(install).toHaveBeenCalledTimes(1)
  })
})

describe('ignore', () => {
  it('stays out of the wire while an install is in flight', async () => {
    const controller = createNotificationsStore(wire({ ignore: never }))
    controller.store.update((state) => { state.installing = true })
    await controller.ignore('v2.0.0')
  })

  it('writes the ignore list and then re-reads the status', async () => {
    const ignored = { ...STATUS_QUIESCENT, ignoredLatest: true }
    const ignore = vi.fn(() => Promise.resolve(ok({ ignoredVersions: ['v2.0.0'] })))
    const status = vi.fn(() => Promise.resolve(ok(ignored)))
    const controller = createNotificationsStore(wire({ ignore, status }))
    await controller.ignore('v2.0.0')
    expect(ignore).toHaveBeenCalledWith({ tag: 'v2.0.0' })
    expect(status).toHaveBeenCalledTimes(1)
    expect(controller.store.getSnapshot().updates).toEqual(ignored)
  })

  it('stops at a rejected write and skips the refresh', async () => {
    const status = vi.fn(() => Promise.resolve(ok(STATUS_QUIESCENT)))
    const controller = createNotificationsStore(wire({ ignore: () => Promise.resolve(fail('refused')), status }))
    await controller.ignore('v2.0.0')
    expect(controller.store.getSnapshot().sdkError).toBe('refused')
    expect(status).not.toHaveBeenCalled()
  })
})

describe('notice mutations', () => {
  it('pulls the list into the snapshot', async () => {
    const rows = [notice(), notice({ read: true })]
    const controller = createNotificationsStore(wire({ list: () => Promise.resolve(ok({ items: rows })) }))
    await controller.refreshNotices()
    expect(controller.store.getSnapshot().notices).toEqual(rows)
  })

  it('keeps the old list and reports the failure on the notices line', async () => {
    const rows = [notice()]
    const list = vi.fn(() => Promise.resolve(ok({ items: rows })))
    const controller = createNotificationsStore(wire({ list }))
    await controller.refreshNotices()
    list.mockImplementation(() => Promise.resolve(fail('list down')))
    await controller.refreshNotices()
    expect(controller.store.getSnapshot()).toMatchObject({ notices: rows, noticesError: 'list down' })
  })

  it('marks one notice read and refetches the list after settle', async () => {
    const afterMutation = [notice({ id: 'n-9', read: true })]
    const setRead = vi.fn(() => Promise.resolve(ok({ ok: true as const })))
    const list = vi.fn(() => Promise.resolve(ok({ items: afterMutation })))
    const controller = createNotificationsStore(wire({ setRead, list }))
    await controller.markRead('n-9')
    expect(setRead).toHaveBeenCalledWith({ id: 'n-9', read: true })
    // The snapshot holds the post-mutation answer, proving the refetch ran
    // after the write rather than before it.
    expect(controller.store.getSnapshot().notices).toEqual(afterMutation)
  })

  it('reports a failed read-marking without a refetch', async () => {
    const list = vi.fn(() => Promise.resolve(ok({ items: [] })))
    const controller = createNotificationsStore(wire({ setRead: () => Promise.resolve(fail('write refused')), list }))
    await controller.markRead('n-1')
    expect(controller.store.getSnapshot().noticesError).toBe('write refused')
    expect(list).not.toHaveBeenCalled()
  })

  it('dismisses one notice and refetches the list after settle', async () => {
    const dismiss = vi.fn(() => Promise.resolve(ok({ ok: true as const })))
    const afterMutation = [notice({ id: 'n-8', dismissed: false })]
    const list = vi.fn(() => Promise.resolve(ok({ items: afterMutation })))
    const controller = createNotificationsStore(wire({ dismiss, list }))
    await controller.dismissNotice('n-8')
    expect(dismiss).toHaveBeenCalledWith({ id: 'n-8' })
    expect(controller.store.getSnapshot().notices).toEqual(afterMutation)
  })

  it('reports a failed dismissal without a refetch', async () => {
    const list = vi.fn(() => Promise.resolve(ok({ items: [] })))
    const controller = createNotificationsStore(wire({
      dismiss: () => Promise.resolve(fail('dismiss refused')),
      list,
    }))
    await controller.dismissNotice('n-1')
    expect(controller.store.getSnapshot().noticesError).toBe('dismiss refused')
    expect(list).not.toHaveBeenCalled()
  })
})

describe('messageOf', () => {
  it('reads an Error message and stringifies anything else', () => {
    expect(messageOf(new Error('boom'))).toBe('boom')
    expect(messageOf('plain refusal')).toBe('plain refusal')
    expect(messageOf(undefined)).toBe('undefined')
  })
})
