/**
 * notifications domain zod schemas (names derived from map keys:
 * notificationsListRequestSchema / notificationsSetValueSchema / …).
 */

import { z } from 'zod'
import type { RequestPayload, ResponseValue } from './rpc-map.ts'
import type { Wire } from './rpc.schema.ts'
import type { NotificationView } from './notifications.ts'

/** NotificationView row of notifications.list. */
export const notificationViewSchema = z.object({
  id: z.string().min(1),
  kind: z.string().min(1),
  title: z.string(),
  body: z.string().optional(),
  data: z.record(z.string(), z.unknown()).optional(),
  createdAt: z.string().min(1),
  dismissed: z.boolean(),
  read: z.boolean(),
}) satisfies z.ZodType<Wire<NotificationView>>

/** notifications.list request payload. */
export const notificationsListRequestSchema = z.object({}) satisfies z.ZodType<Wire<RequestPayload<'notifications.list'>>>

/** notifications.list response value. */
export const notificationsListValueSchema = z.object({
  items: z.array(notificationViewSchema),
}) satisfies z.ZodType<Wire<ResponseValue<'notifications.list'>>>

/** notifications.setRead request payload. */
export const notificationsSetReadRequestSchema = z.object({
  id: z.string().min(1),
  read: z.boolean().optional(),
}) satisfies z.ZodType<Wire<RequestPayload<'notifications.setRead'>>>

/** notifications.setRead response value. */
export const notificationsSetReadValueSchema = z.object({ ok: z.literal(true) }) satisfies z.ZodType<Wire<ResponseValue<'notifications.setRead'>>>

/** notifications.dismiss request payload. */
export const notificationsDismissRequestSchema = z.object({
  id: z.string().min(1),
}) satisfies z.ZodType<Wire<RequestPayload<'notifications.dismiss'>>>

/** notifications.dismiss response value. */
export const notificationsDismissValueSchema = z.object({ ok: z.literal(true) }) satisfies z.ZodType<Wire<ResponseValue<'notifications.dismiss'>>>
