/**
 * Public data types of the sidecar-update service. Runtime code lives in the
 * service module; this file is types only.
 * @module @deepseek-ai/dsh-sidecar-updates/types
 */

/** Platform/arch pair selecting one release asset. */
export interface ReleaseTarget {
  /** Node platform identifier (`darwin`, `linux`, `win32`). */
  platform: NodeJS.Platform
  /** Node architecture identifier (`x64`, `arm64`). */
  arch: string
}

/** One installed release recorded by the pointer document. */
export interface InstalledEntry {
  /** Release tag that was installed. */
  tag: string
  /** Asset filename the binary came from. */
  asset: string
  /** SHA-256 hex digest of the downloaded asset bytes. */
  sha256: string
  /** Completion time of the install as an ISO-8601 string. */
  installedAt: string
  /** Absolute path of the installed executable. */
  exePath: string
}

/** The newest published release, as last observed from a successful check. */
export interface LatestReleaseInfo {
  /** Release tag (`tag_name`), leading `v` retained. */
  tag: string
  /** Release display name, present when the release declared one. */
  name?: string
  /** Publication timestamp from the release, present when declared. */
  publishedAt?: string
  /** Release HTML page URL, present when declared. */
  url?: string
}

/** Cached sync view of the update pipeline's state. */
export interface UpdateStatus {
  /** Currently installed release, or `null` before the first install. */
  installed: InstalledEntry | null
  /** Newest observed release, or `null` before the first successful check. */
  latest: LatestReleaseInfo | null
  /** A newer, non-ignored release is usable for install. */
  updateAvailable: boolean
  /** The newest release is on the ignore list. */
  ignoredLatest: boolean
  /** Message describing the most recent failure, present after one occurred. */
  lastError?: string
}

/** Result of a completed {@linkcode SidecarUpdatesService.install}. */
export interface InstallResult {
  /** Pointer entry committed for the freshly installed release. */
  installed: InstalledEntry
  /** Always `true`: the new binary is picked up at the next engine start. */
  restartRequired: true
}
