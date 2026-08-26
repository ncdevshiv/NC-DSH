/**
 * Auto-update pipeline for the `ai-sidecar` model-engine binary. The service
 * polls a GitHub repository's latest release, stages and checksum-verifies
 * release assets into a versioned install layout, and repoints a
 * `current.json` pointer atomically, so a running executable is never
 * overwritten and every install is all-or-nothing at the rename. Check
 * results surface through the notification seam (`ctx.notifications`) and the
 * `sidecar-updates/status` event; nothing here touches a model request — the
 * new binary is picked up when the engine next starts.
 *
 * The optional `sidecar-updates` settings namespace layers user overrides over
 * this plugin's composition entry config; the section's `ignoredVersions`
 * field is the read-only seed of the ignore list, which
 * {@linkcode SidecarUpdatesService.ignore} grows in an install-directory file.
 * @module @deepseek-ai/dsh-sidecar-updates
 */

import { Context, Service } from '@deepseek-ai/cordis'
import z from '@deepseek-ai/schemastery'
import { join, resolve } from 'node:path'
import type {} from '@deepseek-ai/dsh-notifications'
import { installSettingsSection, settingsNamespace } from '@deepseek-ai/dsh-settings'
import { compareVersions } from './version.ts'
import {
  SidecarUpdateError,
  downloadBytes,
  fetchLatestRelease,
  findSha256SumsAsset,
  parseSha256Sums,
  verifyChecksum,
} from './github.ts'
import type { FetchedRelease, ReleaseAsset } from './github.ts'
import {
  IGNORED_FILENAME,
  POINTER_FILENAME,
  assetNameFor,
  downloadDirFor,
  exePathFor,
  isSupportedTarget,
} from './layout.ts'
import {
  readIgnoredTags,
  readPointer,
  writeAtomicSync,
  writeIgnoredTags,
  writePointer,
} from './persist.ts'
import type { InstalledEntry, InstallResult, LatestReleaseInfo, ReleaseTarget, UpdateStatus } from './types.ts'

export { SidecarUpdateError, REQUEST_TIMEOUT_MS } from './github.ts'
export { compareVersions } from './version.ts'
export {
  INSTALLED_EXE_BASENAME,
  POINTER_FILENAME,
  SHA256SUMS_ASSET,
  assetNameFor,
  exePathFor,
  isSupportedTarget,
} from './layout.ts'
export type { InstalledEntry, InstallResult, LatestReleaseInfo, ReleaseTarget, UpdateStatus }

/** Cordis function-plugin name of the invariant companion's sibling service. */
const SERVICE_KEY = 'sidecarUpdates'

/** Settings namespace layered under the user document. */
const NS = settingsNamespace('sidecar-updates')

/** Default repository polled for releases. */
const DEFAULT_REPO = 'ncdevshiv/ai-sdk'
/** Default GitHub API base. */
const DEFAULT_API_BASE = 'https://api.github.com'
/** Default asset prefix: assets publish as `{prefix}-{platform}-{arch}[.exe]`. */
const DEFAULT_ASSET_PREFIX = 'ai-sidecar'
/** Default install root below the process working directory. */
const DEFAULT_INSTALL_SEGMENTS = ['core-deps', 'ai-sdk'] as const

/** Shortest poll interval the schema admits. */
export const MIN_INTERVAL_MS = 60_000
/** Longest poll interval the schema admits (24 hours). */
export const MAX_INTERVAL_MS = 86_400_000

/** Id prefix of the actionable "update available" notification. */
const UPDATE_NOTIFICATION_PREFIX = 'sdk-update:'
/** Kind of the actionable "update available" notification. */
const UPDATE_NOTIFICATION_KIND = 'sdk-update'

/** Plugin config; every field is optional in yml and resolved by {@link resolveSpec}. */
export interface SidecarUpdatesConfig {
  /** `owner/name` repository polled for releases; default `ncdevshiv/ai-sdk`. */
  repo?: string
  /** Install directory root; default `<cwd>/core-deps/ai-sdk`. */
  installDir?: string
  /** Run one check after startup; default `true`. */
  checkOnStart?: boolean
  /** Poll interval in milliseconds inside [60s, 24h]; omit to disable polling. */
  intervalMs?: number
  /** Release asset prefix; default `ai-sidecar`. */
  assetPrefix?: string
  /** Install the first observed release when nothing is installed yet; default `true`. */
  autoInstallOnFirstRun?: boolean
  /** GitHub API base URL; default `https://api.github.com`. */
  apiBase?: string
  /** Seed of the ignored-release tag list; grown at runtime via {@linkcode SidecarUpdatesService.ignore}. */
  ignoredVersions?: string[]
}

/** Fully resolved pipeline parameters; defaulting happens here, never inline. */
export interface ResolvedSidecarSpec {
  /** Repository slug. */
  repo: string
  /** Absolute install directory root. */
  installDir: string
  /** Whether one check runs after startup. */
  checkOnStart: boolean
  /** Poll interval, or `undefined` when polling is disabled. */
  intervalMs: number | undefined
  /** Asset prefix. */
  assetPrefix: string
  /** Whether the first successful check installs automatically. */
  autoInstallOnFirstRun: boolean
  /** API base without a trailing slash. */
  apiBase: string
}

// Bridged rather than inferred: schemastery materializes optional members in
// its inferred object type, which fights exactOptionalPropertyTypes against
// the hand-written interface. resolveSpec is the typed boundary.
export const Config = z.object({
  repo: z.string().default(DEFAULT_REPO),
  installDir: z.string(),
  checkOnStart: z.boolean().default(true),
  intervalMs: z.number().min(MIN_INTERVAL_MS).max(MAX_INTERVAL_MS),
  assetPrefix: z.string().default(DEFAULT_ASSET_PREFIX),
  autoInstallOnFirstRun: z.boolean().default(true),
  apiBase: z.string().default(DEFAULT_API_BASE),
  ignoredVersions: z.array(z.string()).default([]),
}) as unknown as z<SidecarUpdatesConfig>

/**
 * Resolve raw config to the complete runtime spec. Called for the composition
 * entry at load (fail loud) and per operation for the live settings snapshot;
 * the settings seam itself keeps the last good value when a stored section
 * fails this resolution.
 * @param config - raw plugin config or resolved settings snapshot.
 * @returns the complete spec with every default applied.
 * @throws when a slug or prefix is blank or whitespace-padded, or an explicit
 * base URL ends in a slash after trimming.
 */
export function resolveSpec(config: SidecarUpdatesConfig): ResolvedSidecarSpec {
  const repo = config.repo ?? DEFAULT_REPO
  if (repo.trim().length === 0 || /\s/.test(repo)) {
    throw new Error(`sidecar-updates: repo must be an owner/name slug without whitespace, got ${JSON.stringify(repo)}`)
  }
  const assetPrefix = config.assetPrefix ?? DEFAULT_ASSET_PREFIX
  if (assetPrefix.trim().length === 0 || assetPrefix.includes('/') || assetPrefix.includes('\\')) {
    throw new Error(`sidecar-updates: assetPrefix must be one filename segment without separators, got ${JSON.stringify(assetPrefix)}`)
  }
  const apiBase = (config.apiBase ?? DEFAULT_API_BASE).trim().replace(/\/+$/, '')
  return {
    repo,
    installDir: resolve(config.installDir ?? join(process.cwd(), ...DEFAULT_INSTALL_SEGMENTS)),
    checkOnStart: config.checkOnStart ?? true,
    intervalMs: config.intervalMs,
    assetPrefix,
    autoInstallOnFirstRun: config.autoInstallOnFirstRun ?? true,
    apiBase,
  }
}

/** Process-derived target used unless a subclass overrides the accessor. */
function resolveTarget(): ReleaseTarget {
  return { platform: process.platform, arch: process.arch }
}

declare module '@deepseek-ai/cordis' {
  interface Context {
    sidecarUpdates: SidecarUpdatesService
  }

  interface Events {
    /**
     * Complete pipeline status after every committed check or mutation:
     * check completion, install, and ignore each emit one snapshot.
     * @param status - the frozen full-status payload.
     * @mode emit
     */
    'sidecar-updates/status'(this: SidecarUpdatesService, status: UpdateStatus): void
  }
}

/** GitHub-release update pipeline reporting through the notification seam. */
export class SidecarUpdatesService extends Service {
  static Config = Config
  /** The notification seam receives every check and install outcome. */
  static inject = ['notifications']

  private readonly entryConfig: SidecarUpdatesConfig
  private source: () => SidecarUpdatesConfig
  private latestCache: FetchedRelease | undefined
  private lastErrorValue: string | undefined
  private firstCheckDone = false
  private timer: NodeJS.Timeout | undefined
  private armedIntervalMs: number | undefined
  private hasArmedInterval = false
  private closed = false

  /**
   * @param ctx - Cordis context owning this service; carries `notifications`.
   * @param config - composition entry config, layered under the user settings
   *   section while a settings provider is attached.
   */
  constructor(ctx: Context, config: SidecarUpdatesConfig = {}) {
    super(ctx, SERVICE_KEY)
    // Fail loud before anything registers when the composition entry itself is invalid.
    this.source = () => config
    resolveSpec(config)
    this.entryConfig = config
  }

  /** Resolve the currently authoritative configuration snapshot. */
  private get spec(): ResolvedSidecarSpec {
    return resolveSpec(this.source())
  }

  /** The platform/arch pair selecting release assets; subclasses may pin it. */
  protected get target(): ReleaseTarget {
    return resolveTarget()
  }

  /**
   * Cached sync view of the pipeline state. The pointer and ignore documents
   * are re-read per call, so externally changed files are reflected without
   * cache invalidation.
   * @returns a frozen status snapshot.
   */
  status(): UpdateStatus {
    return Object.freeze(this.buildStatus())
  }

  /**
   * Run one release check now: fetch `releases/latest`, refresh the cached
   * comparison state, reconcile the actionable notification, and — on the
   * first successful check with nothing installed and auto-install enabled —
   * install that release. Transport, HTTP, and parse failures set
   * `lastError`, warn, and never throw.
   * @returns the committed post-check status.
   */
  async checkNow(): Promise<UpdateStatus> {
    const spec = this.spec
    let release: FetchedRelease
    try {
      release = await fetchLatestRelease(spec.apiBase, spec.repo)
    } catch (error) {
      this.lastErrorValue = describeError(error)
      this.ctx.logger.warn('sidecar-updates: release check failed for %s', spec.repo)
      this.ctx.logger.warn(error)
      return this.commitStatus()
    }
    this.latestCache = release
    this.lastErrorValue = undefined
    const firstSuccessfulCheck = !this.firstCheckDone
    this.firstCheckDone = true
    const preCheckStatus = this.buildStatus()
    if (firstSuccessfulCheck && spec.autoInstallOnFirstRun && preCheckStatus.installed === null) {
      try {
        await this.install()
      } catch (error) {
        // Auto-install is background work: record the failure in status
        // instead of failing the check that surfaced the release.
        this.lastErrorValue = describeError(error)
        this.ctx.logger.warn(error)
      }
    }
    this.reconcileUpdateNotification()
    return this.commitStatus()
  }

  /**
   * Download, checksum-verify, and install one release, then atomically
   * repoint the pointer document. Only the published latest release can be
   * installed; the bytes stage under `downloads/<tag>/` and land under
   * `releases/<tag>/`, so a running binary is never overwritten.
   * @param requestedTag - tag to install; defaults to the latest published
   *   release. Any other tag fails with `UNKNOWN_RELEASE`.
   * @returns the committed pointer entry plus `restartRequired`.
   * @throws SidecarUpdateError on lookup, unsupported target, missing asset
   * or checksum manifest, download, or digest mismatch failure.
   */
  async install(requestedTag?: string): Promise<InstallResult> {
    if (this.closed) throw new SidecarUpdateError('DISPOSED', 'the service is disposed')
    try {
      return await this.installInner(requestedTag)
    } catch (error) {
      this.lastErrorValue = describeError(error)
      throw error
    }
  }

  /** Body of one install; {@link install} owns lastError recording. */
  private async installInner(requestedTag?: string): Promise<InstallResult> {
    const spec = this.spec
    const target = this.target
    if (!isSupportedTarget(target)) {
      throw new SidecarUpdateError(
        'UNSUPPORTED_PLATFORM',
        `${target.platform}/${target.arch} publishes no release asset`,
      )
    }
    let release = this.latestCache
    if (release === undefined || (requestedTag !== undefined && requestedTag !== release.tag)) {
      release = await fetchLatestRelease(spec.apiBase, spec.repo)
      this.latestCache = release
    }
    if (requestedTag !== undefined && requestedTag !== release.tag) {
      throw new SidecarUpdateError(
        'UNKNOWN_RELEASE',
        `tag "${requestedTag}" is not the published release "${release.tag}"; only the latest release installs`,
      )
    }
    const assetName = assetNameFor(spec.assetPrefix, target)
    const asset = requireAsset(release.assets, assetName)
    const sumsAsset = findSha256SumsAsset(release.assets)
    if (sumsAsset === undefined) {
      throw new SidecarUpdateError('CHECKSUM_MANIFEST_MISSING', `release ${release.tag} publishes no SHA256SUMS asset`)
    }
    const bytes = await downloadBytes(asset.url)
    const sumsText = new TextDecoder().decode(await downloadBytes(sumsAsset.url))
    const expectedHex = parseSha256Sums(sumsText).get(assetName)
    if (expectedHex === undefined) {
      throw new SidecarUpdateError('CHECKSUM_ENTRY_MISSING', `the SHA256SUMS manifest names no entry for ${assetName}`)
    }
    verifyChecksum(bytes, expectedHex)

    writeAtomicSync(join(downloadDirFor(spec.installDir, release.tag), assetName), bytes)
    const exePath = exePathFor(spec.installDir, target, release.tag)
    writeAtomicSync(exePath, bytes)
    const installed: InstalledEntry = {
      tag: release.tag,
      asset: assetName,
      sha256: expectedHex,
      installedAt: new Date().toISOString(),
      exePath,
    }
    writePointer(join(spec.installDir, POINTER_FILENAME), installed)
    this.lastErrorValue = undefined

    this.ctx.notifications.publish({
      id: `sdk-update-installed:${installed.tag}`,
      kind: 'sdk-update-installed',
      title: `AI SDK ${installed.tag} installed`,
      body: 'Takes effect the next time the model engine starts.',
    })
    this.removeStaleUpdateNotifications(installed.tag)
    this.commitStatus()
    return { installed, restartRequired: true }
  }

  /**
   * Add a release tag to the persisted ignore list, suppressing its
   * "update available" notification until it is removed from
   * `ignored.json` (or the seed in settings). Ignoring an already-ignored
   * tag changes nothing. Persistence is synchronous; the promise confirms
   * the committed state.
   * @param tag - exact release tag to ignore.
   * @returns a promise settling after the list and status are committed.
   */
  ignore(tag: string): Promise<void> {
    if (tag.length === 0 || tag.trim() !== tag) {
      throw new Error(`sidecar-updates: ignore expects one exact release tag, got ${JSON.stringify(tag)}`)
    }
    const spec = this.spec
    const existing = readIgnoredTags(join(spec.installDir, IGNORED_FILENAME))
    if (existing.includes(tag)) return Promise.resolve()
    writeIgnoredTags(join(spec.installDir, IGNORED_FILENAME), [...existing, tag])
    this.reconcileUpdateNotification()
    this.commitStatus()
    return Promise.resolve()
  }

  /**
   * Keep exactly one actionable update notification alive: the current
   * update-available entry is refreshed only when its content changed (so a
   * dismissed notice stays dismissed), and every stale `sdk-update:*` entry
   * is deleted.
   */
  private reconcileUpdateNotification(): void {
    const snap = this.buildStatus()
    const views = this.ctx.notifications.list()
    const desiredId = snap.updateAvailable && snap.latest !== null ? `${UPDATE_NOTIFICATION_PREFIX}${snap.latest.tag}` : null
    if (desiredId !== null && snap.latest !== null) {
      const installedTag = snap.installed?.tag ?? null
      const title = 'AI SDK update available'
      const body = `Installed ${installedTag ?? 'none'} → available ${snap.latest.tag}`
      const data = { tag: snap.latest.tag, url: snap.latest.url ?? null, installed: installedTag }
      const existing = views.find(view => view.id === desiredId)
      const unchanged = existing !== undefined
        && existing.title === title
        && existing.body === body
        && existing.data !== undefined
        && existing.data['tag'] === data.tag
        && existing.data['url'] === data.url
        && existing.data['installed'] === data.installed
      if (!unchanged) {
        this.ctx.notifications.publish({ id: desiredId, kind: UPDATE_NOTIFICATION_KIND, title, body, data })
      }
    }
    for (const view of views) {
      if (!view.id.startsWith(UPDATE_NOTIFICATION_PREFIX)) continue
      if (view.id === desiredId) continue
      this.ctx.notifications.delete(view.id)
    }
  }

  /** Delete the actionable notification for a tag that is now installed. */
  private removeStaleUpdateNotifications(installedTag: string): void {
    for (const view of this.ctx.notifications.list()) {
      if (view.id !== `${UPDATE_NOTIFICATION_PREFIX}${installedTag}`) continue
      this.ctx.notifications.delete(view.id)
    }
  }

  /** Assemble the current status snapshot from live documents and caches. */
  private buildStatus(): UpdateStatus {
    const spec = this.spec
    const installed = readPointer(join(spec.installDir, POINTER_FILENAME))
    const latest: LatestReleaseInfo | null = this.latestCache === undefined ? null : this.latestCache
    const ignored = new Set([
      ...(this.entryConfig.ignoredVersions ?? []),
      ...readIgnoredTags(join(spec.installDir, IGNORED_FILENAME)),
    ])
    const ignoredLatest = latest !== null && ignored.has(latest.tag)
    const updateAvailable = latest !== null && !ignoredLatest
      && (installed === null || compareVersions(latest.tag, installed.tag) > 0)
    return {
      installed,
      latest: latest === null ? null : { ...latest },
      updateAvailable,
      ignoredLatest,
      ...(this.lastErrorValue === undefined ? {} : { lastError: this.lastErrorValue }),
    }
  }

  /** Build, emit, and return the committed status snapshot. */
  private commitStatus(): UpdateStatus {
    const status = Object.freeze(this.buildStatus())
    this.emitStatus(status)
    return status
  }

  /**
   * Fan one committed status snapshot out contained: each listener runs
   * independently, sync throws and async rejections are logged, and an
   * INVARIANT-coded failure still surfaces after every listener ran.
   */
  private emitStatus(status: UpdateStatus): void {
    let invariantFailure: unknown
    for (const listener of this.ctx.events.dispatch('emit', [this, 'sidecar-updates/status', status]) as Array<(status: UpdateStatus) => unknown>) {
      try {
        const returned = listener(status)
        if (returned != null && typeof (returned as PromiseLike<unknown>).then === 'function') {
          void Promise.resolve(returned as PromiseLike<unknown>).then(undefined, (error: unknown) => {
            this.warnListenerFailure(error)
          })
        }
      } catch (error) {
        if ((error as { code?: unknown } | null)?.code === 'INVARIANT') {
          invariantFailure ??= error
          continue
        }
        this.warnListenerFailure(error)
      }
    }
    if (invariantFailure !== undefined) throw invariantFailure as Error
  }

  /** Contained-listener diagnostic shared by the sync and async failure paths. */
  private warnListenerFailure(error: unknown): void {
    this.ctx.logger.warn('sidecar-updates: a sidecar-updates/status listener failed')
    this.ctx.logger.warn(error)
  }

  /** Stop any armed poller and arm a new one when `intervalMs` is configured. */
  private syncInterval(): void {
    const ms = this.spec.intervalMs
    if (this.hasArmedInterval && ms === this.armedIntervalMs) return
    if (this.timer !== undefined) {
      clearInterval(this.timer)
      this.timer = undefined
    }
    this.armedIntervalMs = ms
    this.hasArmedInterval = true
    if (ms === undefined) return
    this.timer = setInterval(() => {
      void this.checkNow().catch((error: unknown) => {
        // checkNow contains its own failures; this guard only covers races
        // between disposal and an in-flight tick.
        this.ctx.logger.warn(error)
      })
    }, ms)
    this.timer.unref()
  }

  [Service.init](): () => void {
    installSettingsSection(this.ctx, NS, Config, this.entryConfig, {
      setSource: (source) => {
        this.source = source
      },
      onChange: () => {
        // Polling follows the live layering; every other fact is resolved
        // per operation, so there is nothing else to re-judge here.
        this.syncInterval()
      },
    })
    this.syncInterval()
    if (this.spec.checkOnStart) {
      void this.checkNow().catch((error: unknown) => {
        // checkNow contains its own failures; this guard only covers races
        // between disposal and the fire-and-forget start check.
        this.ctx.logger.warn(error)
      })
    }
    return () => {
      // Teardown stops the poller first so no tick starts work past disposal.
      this.closed = true
      if (this.timer !== undefined) {
        clearInterval(this.timer)
        this.timer = undefined
      }
    }
  }
}

/** Locate one release asset by exact name, failing loud when absent. */
function requireAsset(assets: readonly ReleaseAsset[], name: string): ReleaseAsset {
  const asset = assets.find(candidate => candidate.name === name)
  if (asset === undefined) {
    throw new SidecarUpdateError('ASSET_MISSING', `the release publishes no asset named ${name}`)
  }
  return asset
}

/** One-line description used for status.lastError. */
function describeError(error: unknown): string {
  /* v8 ignore next -- every failure path in this package throws Error subclasses */
  return error instanceof Error ? error.message : String(error)
}

export default SidecarUpdatesService
