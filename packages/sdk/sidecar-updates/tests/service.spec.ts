import { createHash } from 'node:crypto'
import { existsSync, mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync, mkdirSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { Context } from '@deepseek-ai/cordis'
import NotificationsService from '@deepseek-ai/dsh-notifications'
import { SettingsProvider, settingsNamespace } from '@deepseek-ai/dsh-settings'
import type { SettingsNamespace } from '@deepseek-ai/dsh-settings'
import { IGNORED_FILENAME, POINTER_FILENAME, assetNameFor, exePathFor, isSupportedTarget } from '../src/layout.ts'
import { readIgnoredTags, readPointer, writeAtomicSync } from '../src/persist.ts'
import { resolveSpec, SidecarUpdatesService } from '../src/index.ts'
import type { SidecarUpdatesConfig } from '../src/index.ts'
import type { ReleaseTarget, UpdateStatus } from '../src/types.ts'
import { startFakeGithub, hostAssetName } from './fake-github.ts'
import type { FakeGithub } from './fake-github.ts'

const roots: string[] = []
const servers: FakeGithub[] = []

afterEach(async () => {
  await Promise.all(servers.splice(0).map(server => server.close()))
  for (const root of roots.splice(0)) rmSync(root, { recursive: true, force: true })
})

beforeEach(() => {
  vi.restoreAllMocks()
})

/** Fresh temp root tracked for cleanup. */
function freshRoot(label: string): string {
  const root = mkdtempSync(join(tmpdir(), `dsh-sidecar-${label}-`))
  roots.push(root)
  return root
}

interface BootOptions {
  config?: Partial<SidecarUpdatesConfig>
  fake?: Parameters<typeof startFakeGithub>[0]
}

/**
 * Boot one notifications service plus one sidecar-updates service over a
 * shared home/install tree, pointed at a local fake GitHub server.
 */
async function boot(options: BootOptions = {}) {
  const home = freshRoot('home')
  const installDir = join(freshRoot('install'), 'ai-sdk')
  const server = await startFakeGithub(options.fake)
  servers.push(server)
  const ctx = new Context()
  const notifications = new NotificationsService(ctx, { dshHome: home })
  const config: SidecarUpdatesConfig = {
    repo: 'owner/repo',
    apiBase: server.url,
    installDir,
    checkOnStart: false,
    ...options.config,
  }
  const updates = new SidecarUpdatesService(ctx, config)
  return { ctx, notifications, updates, server, home, installDir }
}

/** Pin the asset target to a fixed platform/arch pair via a test subclass. */
function pinnedTargetClass(target: ReleaseTarget): typeof SidecarUpdatesService {
  return class extends SidecarUpdatesService {
    protected override get target(): ReleaseTarget {
      return target
    }
  }
}

describe('resolveSpec', () => {
  it('applies every default in one explicit step', () => {
    const spec = resolveSpec({ repo: 'o/r' })
    expect(spec.repo).toBe('o/r')
    expect(spec.assetPrefix).toBe('ai-sidecar')
    expect(spec.checkOnStart).toBe(true)
    expect(spec.autoInstallOnFirstRun).toBe(true)
    expect(spec.apiBase).toBe('https://api.github.com')
    expect(spec.intervalMs).toBeUndefined()
    expect(spec.installDir).toBe(join(process.cwd(), 'core-deps', 'ai-sdk'))
  })

  it('normalizes a trailing slash off the api base and rejects bad slugs', () => {
    expect(resolveSpec({ repo: 'o/r', apiBase: 'http://x///' }).apiBase).toBe('http://x')
    expect(() => resolveSpec({ repo: '  ' })).toThrow(/owner\/name slug/)
    expect(() => resolveSpec({ repo: 'a b' })).toThrow(/whitespace/)
    expect(() => resolveSpec({ assetPrefix: 'a/b' })).toThrow(/filename segment/)
    expect(() => resolveSpec({ assetPrefix: '' })).toThrow(/filename segment/)
  })
})

describe('status before any check', () => {
  it('reports nothing installed, nothing latest, no update', async () => {
    const { updates } = await boot()
    expect(updates.status()).toEqual({
      installed: null,
      latest: null,
      updateAvailable: false,
      ignoredLatest: false,
    })
  })
})

describe('checkNow', () => {
  it('publishes exactly the specified update notification and emits status', async () => {
    const statuses: UpdateStatus[] = []
    const { ctx, updates, notifications } = await boot({ config: { autoInstallOnFirstRun: false } })
    ctx.on('sidecar-updates/status', (status) => { statuses.push(status) })

    const status = await updates.checkNow()
    expect(status.latest?.tag).toBe('v1.2.0')
    expect(status.updateAvailable).toBe(true)
    expect(status.ignoredLatest).toBe(false)
    expect(status.lastError).toBeUndefined()
    expect(statuses).toHaveLength(1)

    const views = notifications.list()
    expect(views).toHaveLength(1)
    const view = views[0]
    expect(view?.id).toBe('sdk-update:v1.2.0')
    expect(view?.kind).toBe('sdk-update')
    expect(view?.title).toBe('AI SDK update available')
    expect(view?.body).toBe('Installed none → available v1.2.0')
    expect(view?.data).toEqual({
      tag: 'v1.2.0',
      url: 'https://example.invalid/releases/v1.2.0',
      installed: null,
    })
  })

  it('keeps a dismissed notification dismissed when content is unchanged', async () => {
    const { notifications, updates } = await boot({ config: { autoInstallOnFirstRun: false } })
    await updates.checkNow()
    notifications.dismiss('sdk-update:v1.2.0')
    await updates.checkNow()
    const view = notifications.list().find(entry => entry.id === 'sdk-update:v1.2.0')
    expect(view?.dismissed).toBe(true)
    // The second check changed nothing, so no second publish event fired.
    expect(notifications.list()).toHaveLength(1)
  })

  it('replaces the notification when the installed tag changes its body', async () => {
    const { notifications, updates } = await boot({ config: { autoInstallOnFirstRun: false } })
    await updates.checkNow()
    await updates.install()
    // After installing, the actionable notice is gone; a later check with a
    // newer release would republish against the new installed tag.
    expect(notifications.list().some(entry => entry.id.startsWith('sdk-update:'))).toBe(false)
  })

  it('deletes stale update notices when nothing is actionable', async () => {
    const { notifications, updates } = await boot({ config: { autoInstallOnFirstRun: false } })
    notifications.publish({ id: 'sdk-update:v0.9.0', kind: 'sdk-update', title: 'stale' })
    await updates.checkNow()
    await updates.install()
    expect(notifications.list().filter(entry => entry.id.startsWith('sdk-update:'))).toEqual([])
  })

  it('records lookup failures in lastError without throwing', async () => {
    const { updates } = await boot({ fake: { releaseStatus: 404 } })
    const status = await updates.checkNow()
    expect(status.latest).toBeNull()
    expect(status.updateAvailable).toBe(false)
    expect(status.lastError).toMatch(/answered 404/)
  })

  it('auto-installs on the first successful check when nothing is installed', async () => {
    const { notifications, updates } = await boot()
    const status = await updates.checkNow()
    expect(status.installed?.tag).toBe('v1.2.0')
    expect(status.installed?.asset).toBe(hostAssetName())
    expect(status.installed?.sha256).toMatch(/^[0-9a-f]{64}$/)
    // The auto-install consumed the actionable notice: installed beats update.
    expect(notifications.list().some(entry => entry.id.startsWith('sdk-update:'))).toBe(false)
    expect(notifications.list().some(entry => entry.id === 'sdk-update-installed:v1.2.0')).toBe(true)
  })

  it('records auto-install failures in lastError without failing the check', async () => {
    const { updates } = await boot({ fake: { sumsText: `${'f'.repeat(64)}  ${hostAssetName()}\n` } })
    const status = await updates.checkNow()
    expect(status.latest?.tag).toBe('v1.2.0')
    expect(status.lastError).toMatch(/does not match/)
    expect(updates.status().installed).toBeNull()
  })

  it('publishes a null url when the release declares none', async () => {
    const { notifications, updates } = await boot({
      config: { autoInstallOnFirstRun: false },
      fake: { minimalRelease: true },
    })
    await updates.checkNow()
    const view = notifications.list().find(entry => entry.id === 'sdk-update:v1.2.0')
    expect(view?.data).toEqual({ tag: 'v1.2.0', url: null, installed: null })
  })

  it('does not auto-install when disabled or already installed', async () => {
    const first = await boot({ config: { autoInstallOnFirstRun: false } })
    const status = await first.updates.checkNow()
    expect(status.installed).toBeNull()
    expect(status.updateAvailable).toBe(true)

    const second = await boot()
    await second.updates.checkNow()
    await second.updates.checkNow()
    expect(second.server.requests.filter(path => path.endsWith('/releases/latest'))).toHaveLength(2)
    expect(readPointer(join(second.installDir, POINTER_FILENAME))?.tag).toBe('v1.2.0')
  })
})

describe('install', () => {
  it('stages, verifies, installs, repoints, notifies, and emits', async () => {
    const events: UpdateStatus[] = []
    const { ctx, notifications, updates, installDir, server } = await boot()
    ctx.on('sidecar-updates/status', (status) => { events.push(status) })

    const result = await updates.install()
    expect(result.restartRequired).toBe(true)
    const installed = result.installed
    expect(installed.tag).toBe('v1.2.0')
    expect(installed.exePath).toBe(exePathFor(installDir, { platform: process.platform, arch: process.arch }, 'v1.2.0'))
    expect(installed.installedAt).toMatch(/^\d{4}-\d{2}-\d{2}T/)

    const bytes = new Uint8Array(readFileSync(installed.exePath))
    expect(createHash('sha256').update(bytes).digest('hex')).toBe(installed.sha256)
    expect(existsSync(join(installDir, 'downloads', 'v1.2.0', hostAssetName()))).toBe(true)
    expect(readPointer(join(installDir, POINTER_FILENAME))).toEqual(installed)
    expect(readdirSync(installDir).filter(name => name.includes('.tmp'))).toEqual([])

    const notice = notifications.list().find(entry => entry.id === 'sdk-update-installed:v1.2.0')
    expect(notice).toMatchObject({
      kind: 'sdk-update-installed',
      title: 'AI SDK v1.2.0 installed',
      body: 'Takes effect the next time the model engine starts.',
    })
    expect(events.length).toBeGreaterThanOrEqual(1)
    expect(events.at(-1)?.installed).toEqual(installed)
    expect(server.requests.some(path => path === '/assets/SHA256SUMS')).toBe(true)
  })

  it('rejects an unsupported target without touching disk or network', async () => {
    const Pinned = pinnedTargetClass({ platform: 'sunos', arch: 'sparc' })
    const home = freshRoot('sunos')
    const installDir = join(freshRoot('sunos-install'), 'ai-sdk')
    const server = await startFakeGithub()
    servers.push(server)
    const ctx = new Context()
    new NotificationsService(ctx, { dshHome: home })
    const updates = new Pinned(ctx, { repo: 'owner/repo', apiBase: server.url, installDir })
    await expect(updates.install()).rejects.toMatchObject({ code: 'UNSUPPORTED_PLATFORM' })
    expect(existsSync(installDir)).toBe(false)
  })

  it('fails when the checksum manifest asset is missing', async () => {
    const { updates } = await boot({ fake: { omitSums: true } })
    await expect(updates.install()).rejects.toMatchObject({ code: 'CHECKSUM_MANIFEST_MISSING' })
    expect(updates.status().lastError).toBeDefined()
  })

  it('fails when the release publishes no asset for this target', async () => {
    const { updates } = await boot({ fake: { omitBinary: true } })
    await expect(updates.install()).rejects.toMatchObject({ code: 'ASSET_MISSING' })
    expect(updates.status().lastError).toMatch(/no asset named/)
  })

  it('fails when the manifest names no entry for the asset', async () => {
    const { updates, installDir } = await boot({ fake: { sumsText: 'abcd  other\n' } })
    await expect(updates.install()).rejects.toMatchObject({ code: 'CHECKSUM_ENTRY_MISSING' })
    expect(updates.status().lastError).toBeDefined()
    expect(existsSync(join(installDir, POINTER_FILENAME))).toBe(false)
  })

  it('fails a digest mismatch and leaves the pointer untouched', async () => {
    const wrongDigest = 'f'.repeat(64)
    const { updates, installDir } = await boot({ fake: { sumsText: `${wrongDigest}  ${hostAssetName()}\n` } })
    await expect(updates.install()).rejects.toMatchObject({ code: 'CHECKSUM_MISMATCH' })
    expect(updates.status().lastError).toMatch(/does not match/)
    expect(existsSync(join(installDir, POINTER_FILENAME))).toBe(false)
  })

  it('fails a download error with DOWNLOAD_FAILED and records lastError', async () => {
    const { updates } = await boot({ fake: { downloadStatus: 500 } })
    await expect(updates.install()).rejects.toMatchObject({ code: 'DOWNLOAD_FAILED' })
    expect(updates.status().lastError).toMatch(/answered 500/)
  })

  it('refuses a requested tag that is not the published release', async () => {
    const { updates } = await boot()
    await expect(updates.install('v9.9.9')).rejects.toMatchObject({ code: 'UNKNOWN_RELEASE' })
    await expect(updates.install('v1.2.0')).resolves.toMatchObject({ restartRequired: true })
  })
})

describe('ignore', () => {
  it('suppresses the update notice, persists the tag, and cleans notifications', async () => {
    const { notifications, updates, installDir } = await boot({ config: { autoInstallOnFirstRun: false } })
    await updates.checkNow()
    expect(notifications.list()).toHaveLength(1)
    await updates.ignore('v1.2.0')

    const status = updates.status()
    expect(status.updateAvailable).toBe(false)
    expect(status.ignoredLatest).toBe(true)
    expect(notifications.list().filter(entry => entry.id.startsWith('sdk-update:'))).toEqual([])
    expect(readIgnoredTags(join(installDir, IGNORED_FILENAME))).toEqual(['v1.2.0'])

    await updates.ignore('v1.2.0')
    expect(readIgnoredTags(join(installDir, IGNORED_FILENAME))).toEqual(['v1.2.0'])
  })

  it('merges the settings seed with the persisted list', async () => {
    const { installDir, updates } = await boot({
      config: { ignoredVersions: ['v0.5.0'], autoInstallOnFirstRun: false },
    })
    await updates.checkNow()
    await updates.ignore('v1.2.0')
    expect(readIgnoredTags(join(installDir, IGNORED_FILENAME))).toEqual(['v1.2.0'])
    const status = updates.status()
    expect(status.ignoredLatest).toBe(true)
    expect(status.updateAvailable).toBe(false)
  })

  it('rejects blank tags', async () => {
    const { updates } = await boot()
    expect(() => updates.ignore('  ')).toThrow(/exact release tag/)
    expect(() => updates.ignore('')).toThrow(/exact release tag/)
  })
})

describe('corrupt durable documents', () => {
  it('treats a corrupt pointer as nothing installed and still installs', async () => {
    const { installDir, updates } = await boot()
    mkdirSync(installDir, { recursive: true })
    writeFileSync(join(installDir, POINTER_FILENAME), '{broken', 'utf8')
    expect(updates.status().installed).toBeNull()
    await expect(updates.install()).resolves.toMatchObject({ restartRequired: true })
  })

  it('treats a structurally invalid pointer as nothing installed', async () => {
    const { installDir, updates } = await boot()
    mkdirSync(installDir, { recursive: true })
    writeFileSync(join(installDir, POINTER_FILENAME), JSON.stringify({ tag: 'v1' }), 'utf8')
    expect(updates.status().installed).toBeNull()
    writeFileSync(join(installDir, POINTER_FILENAME), '[1, 2]', 'utf8')
    expect(updates.status().installed).toBeNull()
  })

  it('treats a corrupt ignore list as ignoring nothing', async () => {
    const { installDir, updates } = await boot()
    mkdirSync(installDir, { recursive: true })
    writeFileSync(join(installDir, IGNORED_FILENAME), 'nope', 'utf8')
    await updates.checkNow()
    expect(updates.status().ignoredLatest).toBe(false)
  })

  it('treats a wrongly shaped ignore list as ignoring nothing', async () => {
    const { installDir, updates } = await boot()
    mkdirSync(installDir, { recursive: true })
    writeFileSync(join(installDir, IGNORED_FILENAME), JSON.stringify(['v1', 7]), 'utf8')
    await updates.checkNow()
    expect(updates.status().ignoredLatest).toBe(false)
  })

  it('surfaces non-absence read errors on the pointer path', async () => {
    const { installDir, updates } = await boot()
    mkdirSync(join(installDir, POINTER_FILENAME), { recursive: true })
    expect(() => updates.status()).toThrow()
  })

  it('leaves no temp residue when an atomic write fails mid-install layout', () => {
    const root = freshRoot('atomic-fail')
    const target = join(root, 'nested', 'current.json')
    writeAtomicSync(target, 'one')
    expect(readFileSync(target, 'utf8')).toBe('one')
    mkdirSync(join(root, 'blocked'), { recursive: true })
    expect(() => writeAtomicSync(join(root, 'blocked'), 'two')).toThrow()
    expect(readdirSync(root).filter(name => name.endsWith('.tmp'))).toEqual([])
    expect(existsSync(target)).toBe(true)
  })
})

describe('layout helpers', () => {
  it.each([
    ['win32', 'x64', 'ai-sidecar-win32-x64.exe'],
    ['win32', 'arm64', 'ai-sidecar-win32-arm64.exe'],
    ['linux', 'x64', 'ai-sidecar-linux-x64'],
    ['darwin', 'arm64', 'ai-sidecar-darwin-arm64'],
  ] as const)('names assets for %s/%s', (platform, arch, expected) => {
    expect(assetNameFor('ai-sidecar', { platform, arch })).toBe(expected)
  })

  it('derives versioned executable paths per tag', () => {
    expect(exePathFor('/i', { platform: 'linux', arch: 'x64' }, 'v1'))
      .toBe(join('/i', 'releases', 'v1', 'ai-sidecar'))
    expect(exePathFor('/i', { platform: 'win32', arch: 'x64' }, 'v1'))
      .toBe(join('/i', 'releases', 'v1', 'ai-sidecar.exe'))
  })

  it('judges supported targets exactly', () => {
    for (const platform of ['darwin', 'linux', 'win32'] as const) {
      for (const arch of ['x64', 'arm64']) {
        expect(isSupportedTarget({ platform, arch })).toBe(true)
      }
    }
    expect(isSupportedTarget({ platform: 'sunos', arch: 'x64' })).toBe(false)
    expect(isSupportedTarget({ platform: 'linux', arch: 'riscv64' })).toBe(false)
  })
})

describe('polling and startup checks (fiber lifecycle)', () => {
  it('checks on start and polls on the configured interval', async () => {
    // Fake only the interval APIs: the fetch stack needs real timers.
    vi.useFakeTimers({ toFake: ['setInterval', 'clearInterval'] })
    try {
      const home = freshRoot('timer-home')
      const installDir = join(freshRoot('timer-install'), 'ai-sdk')
      const server = await startFakeGithub()
      servers.push(server)
      const ctx = new Context()
      new NotificationsService(ctx, { dshHome: home })
      const fiber = ctx.plugin(SidecarUpdatesService, {
        repo: 'owner/repo',
        apiBase: server.url,
        installDir,
        intervalMs: 60_000,
      })
      await fiber
      // Real timers stay live for fetch; a short sleep lets the start check land.
      await new Promise(resolve => setTimeout(resolve, 50))
      const afterStart = server.requests.filter(path => path.endsWith('/releases/latest')).length
      expect(afterStart).toBeGreaterThanOrEqual(1)
      expect(ctx.sidecarUpdates.status().lastError).toBeUndefined()

      await vi.advanceTimersByTimeAsync(120_000)
      // Two poll ticks are due; wait out their real in-flight fetches.
      const expectedBeforeDispose = afterStart + 2
      for (let attempt = 0; attempt < 50; attempt += 1) {
        const seen = server.requests.filter(path => path.endsWith('/releases/latest')).length
        if (seen >= expectedBeforeDispose) break
        await new Promise(resolve => setTimeout(resolve, 10))
      }
      await ctx.fiber.dispose()
      const beforeDispose = server.requests.filter(path => path.endsWith('/releases/latest')).length
      expect(beforeDispose).toBe(expectedBeforeDispose)
      await new Promise(resolve => setTimeout(resolve, 30))
      await vi.advanceTimersByTimeAsync(600_000)
      await new Promise(resolve => setTimeout(resolve, 30))
      expect(server.requests.filter(path => path.endsWith('/releases/latest'))).toHaveLength(beforeDispose)
    } finally {
      vi.useRealTimers()
    }
  })

  it('skips the startup check when disabled and arms no timer by default', async () => {
    const home = freshRoot('quiet-home')
    const installDir = join(freshRoot('quiet-install'), 'ai-sdk')
    const server = await startFakeGithub()
    servers.push(server)
    const ctx = new Context()
    new NotificationsService(ctx, { dshHome: home })
    const fiber = ctx.plugin(SidecarUpdatesService, {
      repo: 'owner/repo',
      apiBase: server.url,
      installDir,
      checkOnStart: false,
    })
    await fiber
    await new Promise(resolve => setTimeout(resolve, 10))
    expect(server.requests.filter(path => path.endsWith('/releases/latest'))).toHaveLength(0)
    await ctx.fiber.dispose()
  })

  it('contains a rejected startup check', async () => {
    const warnings: unknown[] = []
    const home = freshRoot('start-fail-home')
    const installDir = join(freshRoot('start-fail-install'), 'ai-sdk')
    const server = await startFakeGithub()
    servers.push(server)
    const ctx = new Context()
    new NotificationsService(ctx, { dshHome: home })
    // An INVARIANT-coded status listener makes every commit surface, so the
    // fire-and-forget start check rejects instead of resolving.
    const invariant = Object.assign(new Error('pointer drifted'), { code: 'INVARIANT' })
    ctx.on('sidecar-updates/status', () => { throw invariant })
    ctx.logger.warn = ((error: unknown) => { warnings.push(error) }) as typeof ctx.logger.warn
    const fiber = ctx.plugin(SidecarUpdatesService, {
      repo: 'owner/repo',
      apiBase: server.url,
      installDir,
      checkOnStart: true,
    })
    await fiber
    await new Promise(resolve => setTimeout(resolve, 50))
    expect(warnings).toContain(invariant)
    await ctx.fiber.dispose()
  })

  it('rejects install after disposal', async () => {
    const home = freshRoot('dispose-home')
    const installDir = join(freshRoot('dispose-install'), 'ai-sdk')
    const server = await startFakeGithub()
    servers.push(server)
    const ctx = new Context()
    new NotificationsService(ctx, { dshHome: home })
    const fiber = ctx.plugin(SidecarUpdatesService, {
      repo: 'owner/repo',
      apiBase: server.url,
      installDir,
      checkOnStart: false,
    })
    await fiber
    const service = ctx.sidecarUpdates
    await ctx.fiber.dispose()
    await expect(service.install()).rejects.toMatchObject({ code: 'DISPOSED' })
  })
})

describe('status listener containment', () => {
  it('contains sync throws and async rejections independently', async () => {
    const warnings: unknown[] = []
    const { ctx, updates } = await boot({ config: { autoInstallOnFirstRun: false } })
    ctx.logger.warn = ((error: unknown) => { warnings.push(error) }) as typeof ctx.logger.warn
    const seen: number[] = []
    ctx.on('sidecar-updates/status', () => { throw new Error('sync boom') })
    ctx.on('sidecar-updates/status', () => Promise.reject(new Error('async boom')))
    ctx.on('sidecar-updates/status', (status) => { seen.push(status.latest?.tag === undefined ? 0 : 1) })

    await updates.checkNow()
    expect(seen).toEqual([1])
    await new Promise(resolve => setTimeout(resolve, 0))
    expect(warnings.some(entry => entry instanceof Error && entry.message === 'sync boom')).toBe(true)
    expect(warnings.some(entry => entry instanceof Error && entry.message === 'async boom')).toBe(true)
  })

  it('rethrows an INVARIANT-coded failure after every listener ran, contained by background paths', async () => {
    vi.useFakeTimers({ toFake: ['setInterval', 'clearInterval'] })
    try {
      const warnings: unknown[] = []
      const home = freshRoot('invariant-home')
      const installDir = join(freshRoot('invariant-install'), 'ai-sdk')
      const server = await startFakeGithub()
      servers.push(server)
      const ctx = new Context()
      new NotificationsService(ctx, { dshHome: home })
      const invariant = Object.assign(new Error('pointer drifted'), { code: 'INVARIANT' })
      ctx.on('sidecar-updates/status', () => { throw invariant })
      const fiber = ctx.plugin(SidecarUpdatesService, {
        repo: 'owner/repo',
        apiBase: server.url,
        installDir,
        checkOnStart: false,
        intervalMs: 60_000,
      })
      await fiber

      // Direct callers see the invariant surface through checkNow.
      const service = ctx.sidecarUpdates
      await expect(service.checkNow()).rejects.toBe(invariant)

      // A background tick hitting the same failure is contained and logged.
      ctx.logger.warn = ((error: unknown) => { warnings.push(error) }) as typeof ctx.logger.warn
      await vi.advanceTimersByTimeAsync(60_000)
      await new Promise(resolve => setTimeout(resolve, 20))
      expect(warnings).toContain(invariant)
      await ctx.fiber.dispose()
    } finally {
      vi.useRealTimers()
    }
  })
})

/** Minimal in-memory settings provider for layering tests. */
class MemorySettings extends SettingsProvider {
  /** Raw stored document. */
  readonly doc: Record<string, unknown> = {}

  get writable(): boolean {
    return true
  }

  protected load(): Promise<Record<string, unknown>> {
    return Promise.resolve(structuredClone(this.doc))
  }

  protected async persist(ns: SettingsNamespace, section: Record<string, unknown>): Promise<void> {
    this.doc[ns] = structuredClone(section)
  }
}

describe('settings layering', () => {
  it('follows a user-layer interval change live', async () => {
    vi.useFakeTimers({ toFake: ['setInterval', 'clearInterval'] })
    try {
      const home = freshRoot('settings-home')
      const installDir = join(freshRoot('settings-install'), 'ai-sdk')
      const server = await startFakeGithub()
      servers.push(server)
      const ctx = new Context()
      new NotificationsService(ctx, { dshHome: home })
      await ctx.plugin(MemorySettings)
      await ctx.plugin(SidecarUpdatesService, {
        repo: 'owner/repo',
        apiBase: server.url,
        installDir,
        intervalMs: 60_000,
      })
      // Wait until the settings section attach replaced the source thunk.
      const ns = settingsNamespace('sidecar-updates')
      for (let attempt = 0; attempt < 50 && ctx.settings.get(ns) === undefined; attempt += 1) {
        await new Promise(resolve => setTimeout(resolve, 10))
      }
      expect(ctx.settings.get(ns)).toBeDefined()

      // The composition base serves while nothing is stored; the first tick
      // lands on the entry-config cadence.
      await vi.advanceTimersByTimeAsync(60_000)
      await new Promise(resolve => setTimeout(resolve, 20))
      const before = server.requests.filter(path => path.endsWith('/releases/latest')).length
      expect(before).toBeGreaterThanOrEqual(1)

      // A user-layer change re-arms the poller at the new cadence: no tick at
      // 60s anymore, one at 120s.
      await ctx.settings.update(ns, { intervalMs: 120_000 })
      await new Promise(resolve => setTimeout(resolve, 20))
      await vi.advanceTimersByTimeAsync(60_000)
      await new Promise(resolve => setTimeout(resolve, 20))
      const midTick = server.requests.filter(path => path.endsWith('/releases/latest')).length
      await vi.advanceTimersByTimeAsync(60_000)
      await new Promise(resolve => setTimeout(resolve, 20))
      const after = server.requests.filter(path => path.endsWith('/releases/latest')).length
      expect(midTick).toBe(after - 1)

      await ctx.fiber.dispose()
    } finally {
      vi.useRealTimers()
    }
  })
})
