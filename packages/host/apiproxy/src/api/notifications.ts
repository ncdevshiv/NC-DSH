/**
 * notifications domain contract: the read face of the dismissible-notification
 * seam (`ctx.notifications`). Publishing and deleting stay host-side — the
 * wire serves listing and per-entry read/dismiss state only, so a browser can
 * render the store without being able to inject entries into it.
 */

import type { RpcRequest, RpcResponse } from './rpc.ts'

/** Wire view of one stored notification (the seam's own immutable read model). */
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
  /** Creation time as an ISO-8601 string; preserved across content replacement. */
  createdAt: string
  /** Whether the user dismissed this entry. */
  dismissed: boolean
  /** Whether this entry was marked read. */
  read: boolean
}

/** Notifications-domain unary methods (the map keys notifications.* of RpcMethodMap). */
export interface NotificationsApi {
  /**
   * Snapshot every live notification, newest first; dismissed entries stay
   * listed with `dismissed: true`.
   */
  list(request: RpcRequest<{}>): Promise<RpcResponse<{ items: NotificationView[] }>>

  /**
   * Mark one notification read or unread. An unknown id fails with
   * `notification-rejected`; setting the current value changes nothing.
   */
  setRead(request: RpcRequest<{ id: string; read?: boolean }>): Promise<RpcResponse<{ ok: true }>>

  /**
   * Dismiss one notification (keeps it listed). An unknown id fails with
   * `notification-rejected`; dismissing twice changes nothing.
   */
  dismiss(request: RpcRequest<{ id: string }>): Promise<RpcResponse<{ ok: true }>>
}
