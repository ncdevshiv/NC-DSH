/**
 * Public data types of the notification seam. Runtime code lives in the
 * service module; this file is types only.
 * @module @deepseek-ai/dsh-notifications/types
 */

/** Content of one notification accepted by {@linkcode NotificationsService.publish}. */
export interface NotificationPublishInput {
  /** Stable identity; publishing again with the same id replaces the content. */
  id: string
  /** Free-form classification consumers use to group or filter entries. */
  kind: string
  /** One-line headline shown to the user. */
  title: string
  /** Optional body text below the title. */
  body?: string
  /** Optional structured payload for consumer-specific rendering. */
  data?: Record<string, unknown>
}

/** Immutable read model of one stored notification. */
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
  /** Whether {@linkcode NotificationsService.setRead} marked this entry read. */
  read: boolean
  /** Whether the user dismissed this entry. */
  dismissed: boolean
}
