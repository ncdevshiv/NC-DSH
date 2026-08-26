/**
 * Notifications store: one snapshot joining the `updates` domain (installed /
 * latest AI SDK release) and the `notifications` domain (system notices). The
 * host stays the single fact source — every mutation writes through the wire
 * and the panel re-renders from the next read, never optimistically.
 *
 * The wire contract is frozen against shapes the API gateway lands in
 * parallel; this package declares the narrowest structural faces locally and
 * reads them off the connection handle. When `IApiClient` grows `updates` and
 * `notifications`, these types are drop-in replaceable by
 * `Pick<IApiClient, 'updates' | 'notifications'>`.
 */

import type { SnapshotStore } from '@deepseek-ai/dsh-client-runtime/client'
import { createSnapshotStore } from '@deepseek-ai/dsh-client-runtime/client'

/** One installed AI SDK release, as the status view reports it. */
export interface UpdateInstalledView {
  /** Installed release tag (`v` prefix retained). */
  tag: string
  /** Asset filename the binary came from. */
  asset: string
  /** SHA-256 hex digest of the downloaded asset bytes. */
  sha256: string
  /** Completion time of the install as an ISO-8601 string. */
  installedAt: string
}

/** The newest published AI SDK release, as the status view reports it. */
export interface UpdateLatestView {
  /** Release tag (`v` prefix retained). */
  tag: string
  /** Release display name, present when the release declared one. */
  name?: string
  /** Publication timestamp, present when declared. */
  publishedAt?: string
  /** Release page URL for the release-notes link. */
  url: string
}

/** Cached view of the update pipeline's state, as the status method answers. */
export interface UpdateStatusView {
  /** Currently installed release, or `null` before the first install. */
  installed: UpdateInstalledView | null
  /** Newest observed release, or `null` before the first successful check. */
  latest: UpdateLatestView | null
  /** A newer, non-ignored release is usable for install. */
  updateAvailable: boolean
  /** The newest release is on the ignore list. */
  ignoredLatest: boolean
  /** Message describing the most recent failure, present after one occurred. */
  lastError?: string
}

/** Immutable read model of one system notice. */
export interface NotificationView {
  /** Stable identity; unique among live entries. */
  id: string
  /** Free-form classification copied from the publish input. */
  kind: string
  /** One-line headline. */
  title: string
  /** Body text, present when published with one. */
  body?: string
  /** Structured payload, present when published with one. */
  data?: Record<string, unknown>
  /** Creation time as an ISO-8601 string. */
  createdAt: string
  /** Whether the user dismissed this entry. */
  dismissed: boolean
  /** Whether the user read this entry. */
  read: boolean
}

/**
 * Wire result envelope (structural mirror of the carrier's result union; a
 * business rejection names its reason in `error.message`).
 */
export type ApiResult<T> =
  | { ok: true; value: T }
  | { ok: false; error: { message: string } }

/** Response envelope (structural mirror of the carrier's response record). */
export interface ApiResponse<T> {
  result: ApiResult<T>
}

/** The frozen `updates` wire face the panel drives. */
export interface UpdatesFace {
  /** Current cached status. */
  status(): Promise<ApiResponse<UpdateStatusView>>
  /** Force a freshness check, then answer with the resulting status. */
  check(): Promise<ApiResponse<UpdateStatusView>>
  /** Download and stage a release; the new binary takes effect next start. */
  install(payload: { tag?: string }): Promise<ApiResponse<{ installed: UpdateInstalledView; restartRequired: boolean }>>
  /** Add a release to the ignore list. */
  ignore(payload: { tag: string }): Promise<ApiResponse<{ ignoredVersions: string[] }>>
}

/** The frozen `notifications` wire face the panel drives. */
export interface NotificationsFace {
  /** Every live notice, newest first. */
  list(): Promise<ApiResponse<{ items: NotificationView[] }>>
  /** Mark one notice read or unread. */
  setRead(payload: { id: string; read?: boolean }): Promise<ApiResponse<{ ok: true }>>
  /** Dismiss one notice. */
  dismiss(payload: { id: string }): Promise<ApiResponse<{ ok: true }>>
}

/** The connection-handle slice this plugin reads (frozen contract). */
export type UpdatesNotificationsApi = UpdatesFace & NotificationsFace

/** Panel snapshot rendered through the bound snapshot hook. */
export interface NotificationsState {
  /** Latest status view; `null` until the first successful status read settles. */
  updates: UpdateStatusView | null
  /** Notices as the host lists them (dismissed entries included; filtered at render). */
  notices: readonly NotificationView[]
  /** An install is in flight; every card action is disabled while true. */
  installing: boolean
  /** An explicit check is in flight; the check button shows its busy label. */
  checking: boolean
  /**
   * Tag installed during this controller's lifetime whose success copy is
   * showing. Cleared once a later status reports a new version available.
   */
  installedNow: string | null
  /**
   * Inline error line of the updates domain (status, check, install, ignore);
   * `null` after that domain's last operation succeeded.
   */
  sdkError: string | null
  /**
   * Inline error line of the notices domain (list, setRead, dismiss); `null`
   * after that domain's last operation succeeded.
   */
  noticesError: string | null
}

/**
 * Human text for any rejection value: a transport failure rejects with an
 * Error, a host can reject with anything, and the line still has to render.
 * @param error - the rejection value.
 * @returns the message to show.
 */
export function messageOf(error: unknown): string {
  return error instanceof Error ? error.message : String(error)
}

const INITIAL_STATE: NotificationsState = {
  updates: null,
  notices: [],
  installing: false,
  checking: false,
  installedNow: null,
  sdkError: null,
  noticesError: null,
}

/**
 * Create the notifications controller: the snapshot store plus the complete
 * write surface. Production calls this once inside apply; tests call it
 * directly (the sanctioned zero-machinery path). Every method settles its own
 * failures into the inline error line — nothing throws to the component.
 * @param api - the frozen updates/notifications wire face.
 * @returns the store handle and its mutation methods.
 */
export function createNotificationsStore(api: UpdatesNotificationsApi): {
  /** uSES-safe state source bound as the panel's snapshot hook. */
  store: SnapshotStore<NotificationsState>
  /** Silent background sync of the update status (poll and focus refetches). */
  refreshStatus(): Promise<void>
  /** Explicit freshness check; drives the check button's busy label. */
  checkNow(): Promise<void>
  /** Install a release tag, then re-read the status. */
  install(tag: string): Promise<void>
  /** Ignore a release tag, then re-read the status. */
  ignore(tag: string): Promise<void>
  /** Pull the notice list (panel open and after every notice mutation). */
  refreshNotices(): Promise<void>
  /** Mark one notice read, then re-read the list. */
  markRead(id: string): Promise<void>
  /** Dismiss one notice, then re-read the list. */
  dismissNotice(id: string): Promise<void>
} {
  const store = createSnapshotStore<NotificationsState>({ ...INITIAL_STATE })

  const applyStatus = (status: ApiResult<UpdateStatusView>): void => {
    if (!status.ok) {
      store.update((state) => { state.sdkError = status.error.message })
      return
    }
    store.update((state) => {
      state.updates = status.value
      state.sdkError = null
      // The success copy describes exactly the release just staged; a newer
      // arrival reopens the ordinary offer.
      if (status.value.updateAvailable) state.installedNow = null
    })
  }

  const runStatusRead = async (read: () => Promise<ApiResponse<UpdateStatusView>>): Promise<void> => {
    try {
      applyStatus((await read()).result)
    } catch (error) {
      store.update((state) => { state.sdkError = messageOf(error) })
    }
  }

  const refreshStatus = async (): Promise<void> => { await runStatusRead(() => api.status()) }

  const refreshNotices = async (): Promise<void> => {
    try {
      const response = await api.list()
      if (!response.result.ok) throw new Error(response.result.error.message)
      const items = response.result.value.items
      store.update((state) => {
        state.notices = items
        state.noticesError = null
      })
    } catch (error) {
      store.update((state) => { state.noticesError = messageOf(error) })
    }
  }

  return {
    store,
    refreshStatus,
    async checkNow() {
      if (store.getSnapshot().checking) return
      store.update((state) => { state.checking = true })
      await runStatusRead(() => api.check())
      store.update((state) => { state.checking = false })
    },
    async install(tag: string) {
      if (store.getSnapshot().installing) return
      store.update((state) => { state.installing = true; state.sdkError = null })
      try {
        const response = await api.install({ tag })
        // A local const keeps the discriminated narrowing usable below.
        const result = response.result
        if (!result.ok) throw new Error(result.error.message)
        const stagedTag = result.value.installed.tag
        store.update((state) => { state.installedNow = stagedTag })
      } catch (error) {
        // A refused or failed write changed no server fact; stop here so the
        // inline error survives instead of being cleared by a fresh read.
        store.update((state) => { state.sdkError = messageOf(error); state.installing = false })
        return
      }
      store.update((state) => { state.installing = false })
      // Re-read after settle: the card swaps to the success copy from real
      // server state, never from the request alone.
      await refreshStatus()
    },
    async ignore(tag: string) {
      if (store.getSnapshot().installing) return
      try {
        const response = await api.ignore({ tag })
        if (!response.result.ok) throw new Error(response.result.error.message)
      } catch (error) {
        store.update((state) => { state.sdkError = messageOf(error) })
        return
      }
      await refreshStatus()
    },
    refreshNotices,
    async markRead(id: string) {
      try {
        const response = await api.setRead({ id, read: true })
        if (!response.result.ok) throw new Error(response.result.error.message)
      } catch (error) {
        store.update((state) => { state.noticesError = messageOf(error) })
        return
      }
      await refreshNotices()
    },
    async dismissNotice(id: string) {
      try {
        const response = await api.dismiss({ id })
        if (!response.result.ok) throw new Error(response.result.error.message)
      } catch (error) {
        store.update((state) => { state.noticesError = messageOf(error) })
        return
      }
      await refreshNotices()
    },
  }
}

/** Inferred controller type; components and inject factories name this. */
export type NotificationsController = ReturnType<typeof createNotificationsStore>
