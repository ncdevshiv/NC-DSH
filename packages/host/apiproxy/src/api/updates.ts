/**
 * updates domain contract: the web face of the sidecar auto-update seam
 * (`ctx.sidecarUpdates`). Status views are projections decided here: the
 * installed entry's `exePath` is a host-internal fact and never crosses the
 * wire, in any response or frame.
 */

import type { RpcRequest, RpcResponse } from './rpc.ts'

/** Wire view of one installed release: the pointer entry without its `exePath`. */
export interface InstalledUpdateView {
  /** Release tag that was installed. */
  tag: string
  /** Asset filename the binary came from. */
  asset: string
  /** SHA-256 hex digest of the downloaded asset bytes. */
  sha256: string
  /** Completion time of the install as an ISO-8601 string. */
  installedAt: string
}

/** Wire view of the newest published release, as last observed by a check. */
export interface LatestReleaseView {
  /** Release tag, leading `v` retained. */
  tag: string
  /** Release display name, present when the release declared one. */
  name?: string
  /** Publication timestamp from the release, present when declared. */
  publishedAt?: string
  /** Release page URL, present when declared. */
  url?: string
}

/**
 * Wire view of the pipeline state after every committed check or mutation.
 * Also the payload of the `updates/status` host-stream frame, which carries
 * this projection of the seam's `sidecar-updates/status` owner event.
 */
export interface UpdateStatusView {
  /** Currently installed release, or `null` before the first install. */
  installed: InstalledUpdateView | null
  /** Newest observed release, or `null` before the first successful check. */
  latest: LatestReleaseView | null
  /** A newer, non-ignored release is usable for install. */
  updateAvailable: boolean
  /** The newest release is on the ignore list. */
  ignoredLatest: boolean
  /** Message describing the most recent failure, present after one occurred. */
  lastError?: string
}

/** Updates-domain unary methods (the map keys updates.* of RpcMethodMap). */
export interface UpdatesApi {
  /**
   * Cached sync view of the pipeline state; the pointer and ignore documents
   * are re-read per call, so externally changed files are reflected.
   */
  status(request: RpcRequest<{}>): Promise<RpcResponse<UpdateStatusView>>

  /**
   * Run one release check now and answer the committed post-check status.
   * Transport, HTTP, and parse failures set `lastError` instead of failing
   * the call — the seam never throws from a check.
   */
  check(request: RpcRequest<{}>): Promise<RpcResponse<UpdateStatusView>>

  /**
   * Download, checksum-verify, and install one release, then repoint the
   * install. Only the published latest release installs; any other tag fails
   * with `update-failed`. The reply's `installed` view strips `exePath`, and
   * `restartRequired` is always `true`: the new binary starts with the engine.
   */
  install(request: RpcRequest<{ tag?: string }>): Promise<RpcResponse<{ installed: InstalledUpdateView; restartRequired: true }>>

  /**
   * Add one exact release tag to the persisted ignore list, suppressing its
   * "update available" notice. `ignoredVersions` carries the tags this call
   * committed; the seam serves no enumeration of the whole list.
   */
  ignore(request: RpcRequest<{ tag: string }>): Promise<RpcResponse<{ ignoredVersions: string[] }>>
}
