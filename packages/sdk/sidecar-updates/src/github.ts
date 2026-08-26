/**
 * GitHub Releases wire client of the sidecar-update pipeline: one unauthenticated
 * lookup of a repository's latest release, asset download, SHA256SUMS parsing,
 * and digest verification. Every failure surfaces as a {@link SidecarUpdateError}
 * carrying a stable machine code; responses are validated at this wire boundary
 * before any caller sees them.
 * @module @deepseek-ai/dsh-sidecar-updates/github
 */

import { createHash } from 'node:crypto'
import type { LatestReleaseInfo } from './types.ts'
import { SHA256SUMS_ASSET } from './layout.ts'

/**
 * Typed failure of the update pipeline. Background paths report these through
 * `status.lastError`; only install() rejects with one to its direct caller.
 */
export class SidecarUpdateError extends Error {
  /** Stable machine code (`RELEASE_LOOKUP`, `CHECKSUM_MISMATCH`, ...). */
  readonly code: string

  /**
   * @param code - stable machine code for callers that branch on failures.
   * @param message - human-readable failure summary.
   * @param options - optional underlying cause.
   */
  constructor(code: string, message: string, options?: ErrorOptions) {
    super(`sidecar-updates (${code}): ${message}`, options)
    this.name = 'SidecarUpdateError'
    this.code = code
  }
}

/** Per-request deadline applied to every GitHub fetch. */
export const REQUEST_TIMEOUT_MS = 30_000

/** One downloadable release artifact. */
export interface ReleaseAsset {
  /** Filename as published on the release. */
  name: string
  /** Absolute download URL. */
  url: string
}

/** The latest release of a repository, validated down to the fields used here. */
export interface FetchedRelease extends LatestReleaseInfo {
  /** Downloadable assets with both a name and a URL; malformed rows are dropped. */
  assets: ReleaseAsset[]
}

/**
 * Fetch and validate `releases/latest` for one repository.
 * @param apiBase - GitHub API base URL without a trailing slash.
 * @param repo - `owner/name` repository slug.
 * @returns the validated release.
 * @throws SidecarUpdateError with code `RELEASE_LOOKUP` on transport or HTTP
 * failure, or `RELEASE_MALFORMED` when the body lacks a usable tag or asset list.
 */
export async function fetchLatestRelease(apiBase: string, repo: string): Promise<FetchedRelease> {
  let response: Response
  try {
    response = await fetch(`${apiBase}/repos/${repo}/releases/latest`, {
      headers: { accept: 'application/vnd.github+json' },
      signal: AbortSignal.timeout(REQUEST_TIMEOUT_MS),
    })
  } catch (error) {
    throw new SidecarUpdateError('RELEASE_LOOKUP', `request to ${apiBase} failed`, { cause: error })
  }
  if (!response.ok) {
    throw new SidecarUpdateError('RELEASE_LOOKUP', `${apiBase}/repos/${repo}/releases/latest answered ${String(response.status)}`)
  }
  let body: unknown
  try {
    body = await response.json()
  } catch (error) {
    throw new SidecarUpdateError('RELEASE_MALFORMED', 'release body is not JSON', { cause: error })
  }
  return parseRelease(body)
}

/**
 * Validate one parsed release document down to the fields this pipeline uses.
 * @param body - the JSON-parsed response body.
 * @returns the validated release.
 * @throws SidecarUpdateError with code `RELEASE_MALFORMED` when `tag_name` or
 * the asset list is unusable.
 */
export function parseRelease(body: unknown): FetchedRelease {
  if (typeof body !== 'object' || body === null || Array.isArray(body)) {
    throw new SidecarUpdateError('RELEASE_MALFORMED', 'release document is not an object')
  }
  const record = body as Record<string, unknown>
  if (typeof record['tag_name'] !== 'string' || record['tag_name'].length === 0) {
    throw new SidecarUpdateError('RELEASE_MALFORMED', 'release document carries no tag_name')
  }
  if (!Array.isArray(record['assets'])) {
    throw new SidecarUpdateError('RELEASE_MALFORMED', 'release document carries no asset list')
  }
  const assets: ReleaseAsset[] = []
  for (const row of record['assets']) {
    if (typeof row !== 'object' || row === null) continue
    const asset = row as Record<string, unknown>
    if (typeof asset['name'] !== 'string' || typeof asset['browser_download_url'] !== 'string') continue
    assets.push({ name: asset['name'], url: asset['browser_download_url'] })
  }
  return {
    tag: record['tag_name'],
    ...(typeof record['name'] === 'string' && record['name'].length > 0 ? { name: record['name'] } : {}),
    ...(typeof record['published_at'] === 'string' && record['published_at'].length > 0
      ? { publishedAt: record['published_at'] }
      : {}),
    ...(typeof record['html_url'] === 'string' && record['html_url'].length > 0 ? { url: record['html_url'] } : {}),
    assets,
  }
}

/**
 * Download one asset's complete byte content.
 * @param url - absolute asset URL.
 * @returns the asset bytes.
 * @throws SidecarUpdateError with code `DOWNLOAD_FAILED` on transport or HTTP failure.
 */
export async function downloadBytes(url: string): Promise<Uint8Array> {
  let response: Response
  try {
    response = await fetch(url, { signal: AbortSignal.timeout(REQUEST_TIMEOUT_MS) })
  } catch (error) {
    throw new SidecarUpdateError('DOWNLOAD_FAILED', `download from ${url} failed`, { cause: error })
  }
  if (!response.ok) {
    throw new SidecarUpdateError('DOWNLOAD_FAILED', `download from ${url} answered ${String(response.status)}`)
  }
  return new Uint8Array(await response.arrayBuffer())
}

/**
 * Parse a SHA256SUMS manifest: whitespace-separated `digest filename` lines,
 * with an optional binary-mode `*` marker. Blank and unparsable lines are
 * skipped; digests are normalized to lowercase hex.
 * @param text - manifest content.
 * @returns digest per filename, lowercased.
 */
export function parseSha256Sums(text: string): ReadonlyMap<string, string> {
  const digests = new Map<string, string>()
  for (const line of text.split('\n')) {
    const match = /^([0-9a-fA-F]{64})\s+\*?(.+?)\s*$/.exec(line.trim())
    if (match === null) continue
    digests.set(match[2] as string, (match[1] as string).toLowerCase())
  }
  return digests
}

/**
 * Verify downloaded bytes against an expected digest.
 * @param bytes - downloaded asset bytes.
 * @param expectedHex - expected lowercase SHA-256 hex digest.
 * @throws SidecarUpdateError with code `CHECKSUM_MISMATCH` on any difference.
 */
export function verifyChecksum(bytes: Uint8Array, expectedHex: string): void {
  const actual = createHash('sha256').update(bytes).digest('hex')
  if (actual !== expectedHex.toLowerCase()) {
    throw new SidecarUpdateError(
      'CHECKSUM_MISMATCH',
      `asset digest ${actual} does not match the manifest entry ${expectedHex}`,
    )
  }
}

/**
 * Locate the checksum-manifest asset among a release's assets.
 * @param assets - the release's validated assets.
 * @returns the `SHA256SUMS` asset, or `undefined` when absent.
 */
export function findSha256SumsAsset(assets: readonly ReleaseAsset[]): ReleaseAsset | undefined {
  return assets.find(asset => asset.name === SHA256SUMS_ASSET)
}
