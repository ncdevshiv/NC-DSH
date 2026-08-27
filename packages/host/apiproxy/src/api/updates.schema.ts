/**
 * updates domain zod schemas (names derived from map keys:
 * updatesStatusRequestSchema / updatesStatusValueSchema / …). The status view
 * schema has no `exePath` field by design: an install entry that carries one
 * fails the client-side parse rather than leaking the host path.
 */

import { z } from 'zod'
import type { RequestPayload, ResponseValue } from './rpc-map.ts'
import type { Wire } from './rpc.schema.ts'
import type { InstalledUpdateView, LatestReleaseView, UpdateStatusView } from './updates.ts'

/** Exact release tag: the seam's own guard (non-empty, no surrounding whitespace). */
const releaseTagSchema = z.string().min(1).refine(tag => tag.trim() === tag)

/**
 * InstalledUpdateView row of updates.status. Strict on purpose: the exePath
 * strip is this domain's security invariant, so an entry still carrying the
 * host path must fail the parse, not pass through silently stripped.
 */
export const installedUpdateViewSchema = z.strictObject({
  tag: z.string().min(1),
  asset: z.string().min(1),
  sha256: z.string().min(1),
  installedAt: z.string().min(1),
}) satisfies z.ZodType<Wire<InstalledUpdateView>>

/** LatestReleaseView row of updates.status. */
export const latestReleaseViewSchema = z.object({
  tag: z.string().min(1),
  name: z.string().optional(),
  publishedAt: z.string().optional(),
  url: z.string().optional(),
}) satisfies z.ZodType<Wire<LatestReleaseView>>

/** UpdateStatusView value shared by updates.status/updates.check and the updates/status frame. */
export const updateStatusViewSchema = z.object({
  installed: installedUpdateViewSchema.nullable(),
  latest: latestReleaseViewSchema.nullable(),
  updateAvailable: z.boolean(),
  ignoredLatest: z.boolean(),
  lastError: z.string().optional(),
}) satisfies z.ZodType<Wire<UpdateStatusView>>

/** updates.status request payload. */
export const updatesStatusRequestSchema = z.object({}) satisfies z.ZodType<Wire<RequestPayload<'updates.status'>>>

/** updates.status response value. */
export const updatesStatusValueSchema = updateStatusViewSchema satisfies z.ZodType<Wire<ResponseValue<'updates.status'>>>

/** updates.check request payload. */
export const updatesCheckRequestSchema = z.object({}) satisfies z.ZodType<Wire<RequestPayload<'updates.check'>>>

/** updates.check response value. */
export const updatesCheckValueSchema = updateStatusViewSchema satisfies z.ZodType<Wire<ResponseValue<'updates.check'>>>

/** updates.install request payload. */
export const updatesInstallRequestSchema = z.object({
  tag: releaseTagSchema.optional(),
}) satisfies z.ZodType<Wire<RequestPayload<'updates.install'>>>

/** updates.install response value. */
export const updatesInstallValueSchema = z.object({
  installed: installedUpdateViewSchema,
  restartRequired: z.literal(true),
}) satisfies z.ZodType<Wire<ResponseValue<'updates.install'>>>

/** updates.ignore request payload. */
export const updatesIgnoreRequestSchema = z.object({
  tag: releaseTagSchema,
}) satisfies z.ZodType<Wire<RequestPayload<'updates.ignore'>>>

/** updates.ignore response value. */
export const updatesIgnoreValueSchema = z.object({
  ignoredVersions: z.array(z.string().min(1)),
}) satisfies z.ZodType<Wire<ResponseValue<'updates.ignore'>>>
