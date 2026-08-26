/**
 * Shared frozen-wire fixtures for the notifications suites: envelope builders,
 * view factories, and a programmable face whose every method is a spy.
 */

import { vi, type Mock } from 'vitest'
import type {
  ApiResponse, NotificationView, UpdateInstalledView, UpdateStatusView,
  UpdatesFace, NotificationsFace,
} from '../src/client/store.ts'

let nextId = 0

/** Successful envelope. */
export function ok<T>(value: T): ApiResponse<T> {
  return { result: { ok: true, value } }
}

/** Rejected envelope carrying a host-side reason. */
export function fail<T>(message: string): ApiResponse<T> {
  return { result: { ok: false, error: { message } } }
}

/** An answer that never settles; holds a surface on its loading state. */
export function hold<T>(): Promise<T> {
  return new Promise<T>(() => {})
}

/** A status answer that never settles; holds the card on its placeholders. */
export const holdStatus = (): Promise<ApiResponse<UpdateStatusView>> => hold()

/** One installed-release view. */
export function installedView(over: Partial<UpdateInstalledView> = {}): UpdateInstalledView {
  return {
    tag: 'v1.2.3',
    asset: 'dsh-sdk-v1.2.3.tar.gz',
    sha256: 'a'.repeat(64),
    installedAt: '2026-08-01T00:00:00.000Z',
    ...over,
  }
}

/** Quiescent status: everything current, nothing to do. */
export const STATUS_QUIESCENT: UpdateStatusView = {
  installed: installedView(),
  latest: { tag: 'v1.2.3', url: 'https://example.test/releases/v1.2.3' },
  updateAvailable: false,
  ignoredLatest: false,
}

/** Status offering an upgrade. */
export function statusOffering(latestTag = 'v2.0.0'): UpdateStatusView {
  return {
    installed: installedView(),
    latest: { tag: latestTag, name: `AI SDK ${latestTag}`, url: `https://example.test/releases/${latestTag}` },
    updateAvailable: true,
    ignoredLatest: false,
  }
}

/** One notice row, newest-first ordering left to the host's list order. */
export function notice(over: Partial<NotificationView> = {}): NotificationView {
  nextId += 1
  return {
    id: `n-${nextId}`,
    kind: 'system',
    title: `通知 ${nextId}`,
    createdAt: '2026-08-26T11:00:00.000Z',
    dismissed: false,
    read: false,
    ...over,
  }
}

/** Install receipt used by the default install spy. */
function defaultInstallReceipt(): ApiResponse<{ installed: UpdateInstalledView; restartRequired: true }> {
  return ok({
    installed: installedView({ tag: 'v2.0.0', installedAt: '2026-08-26T12:00:00.000Z' }),
    restartRequired: true,
  })
}

/** The exact spy signatures the frozen faces demand. */
export interface WireSpies {
  status: Mock<() => Promise<ApiResponse<UpdateStatusView>>>
  check: Mock<() => Promise<ApiResponse<UpdateStatusView>>>
  install: Mock<() => Promise<ApiResponse<{ installed: UpdateInstalledView; restartRequired: true }>>>
  ignore: Mock<() => Promise<ApiResponse<{ ignoredVersions: string[] }>>>
  list: Mock<() => Promise<ApiResponse<{ items: NotificationView[] }>>>
  setRead: Mock<() => Promise<ApiResponse<{ ok: true }>>>
  dismiss: Mock<() => Promise<ApiResponse<{ ok: true }>>>
}

export interface WireFixtures {
  status?: ApiResponse<UpdateStatusView>
  check?: ApiResponse<UpdateStatusView>
  install?: ApiResponse<{ installed: UpdateInstalledView; restartRequired: true }>
  ignore?: ApiResponse<{ ignoredVersions: string[] }>
  list?: ApiResponse<{ items: NotificationView[] }>
  setRead?: ApiResponse<{ ok: true }>
  dismiss?: ApiResponse<{ ok: true }>
}

/**
 * Build the two-domain face as spies with fixture-backed answers. Fixtures
 * fall through (`check` → `status`, everything else → a benign default), so a
 * suite stubs only what it asserts.
 */
export function fakeWire(fixtures: WireFixtures = {}): UpdatesFace & NotificationsFace & WireSpies {
  const status: WireSpies['status'] = vi.fn()
  status.mockImplementation(() => Promise.resolve(fixtures.status ?? ok(STATUS_QUIESCENT)))
  const check: WireSpies['check'] = vi.fn()
  check.mockImplementation(() => Promise.resolve(fixtures.check ?? fixtures.status ?? ok(STATUS_QUIESCENT)))
  const install: WireSpies['install'] = vi.fn()
  install.mockImplementation(() => Promise.resolve(fixtures.install ?? defaultInstallReceipt()))
  const ignore: WireSpies['ignore'] = vi.fn()
  ignore.mockImplementation(() => Promise.resolve(fixtures.ignore ?? ok({ ignoredVersions: ['v2.0.0'] })))
  const list: WireSpies['list'] = vi.fn()
  list.mockImplementation(() => Promise.resolve(fixtures.list ?? ok({ items: [] as NotificationView[] })))
  const setRead: WireSpies['setRead'] = vi.fn()
  setRead.mockImplementation(() => Promise.resolve(fixtures.setRead ?? ok({ ok: true })))
  const dismiss: WireSpies['dismiss'] = vi.fn()
  dismiss.mockImplementation(() => Promise.resolve(fixtures.dismiss ?? ok({ ok: true })))
  return { status, check, install, ignore, list, setRead, dismiss }
}
