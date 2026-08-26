import { mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterEach, describe, expect, it } from 'vitest'
import { Context } from '@deepseek-ai/cordis'
import { InvariantError, InvariantRegistry } from '@deepseek-ai/dsh-invariants'
import NotificationsService from '@deepseek-ai/dsh-notifications'
import { SidecarUpdatesService } from '../src/index.ts'
import * as SidecarUpdatesInvariant from '../src/invariant.ts'
import { startFakeGithub } from './fake-github.ts'
import type { FakeGithub } from './fake-github.ts'

const roots: string[] = []
const contexts: Context[] = []
const servers: FakeGithub[] = []

afterEach(async () => {
  await Promise.all(contexts.splice(0).map(ctx => ctx.fiber.dispose()))
  for (const root of roots.splice(0)) rmSync(root, { recursive: true, force: true })
  await Promise.all(servers.splice(0).map(server => server.close()))
})

/** Boot the registry, the companion, and one real pipeline over a fake server. */
async function setup() {
  const ctx = new Context()
  contexts.push(ctx)
  const home = mkdtempSync(join(tmpdir(), 'dsh-sidecar-invariant-home-'))
  const installDir = join(mkdtempSync(join(tmpdir(), 'dsh-sidecar-invariant-install-')), 'ai-sdk')
  roots.push(home, join(installDir, '..'))
  const server = await startFakeGithub()
  servers.push(server)
  await ctx.plugin(InvariantRegistry)
  await ctx.plugin(SidecarUpdatesInvariant)
  await ctx.plugin(NotificationsService, { dshHome: home })
  await ctx.plugin(SidecarUpdatesService, {
    repo: 'owner/repo',
    apiBase: server.url,
    installDir,
    checkOnStart: false,
  })
  return { ctx, updates: ctx.sidecarUpdates, installDir }
}

describe('sidecar-updates invariants', () => {
  it('accepts a committed status whose pointer names an existing binary', async () => {
    const { updates } = await setup()
    // The install commit emits through the invariant listener; a violation
    // would reject install() itself.
    const result = await updates.install()
    expect(result.installed.exePath).toBeTruthy()
  })

  it('fails a status whose pointer names a missing executable', async () => {
    const { ctx, installDir } = await setup()
    const forged = Object.freeze({
      installed: {
        tag: 'v0.0.1',
        asset: 'ai-sidecar',
        sha256: '0'.repeat(64),
        installedAt: '2026-08-26T00:00:00.000Z',
        exePath: join(installDir, 'absent-binary'),
      },
      latest: null,
      updateAvailable: false,
      ignoredLatest: false,
    })
    expect(() => { ctx.emit(ctx.sidecarUpdates, 'sidecar-updates/status', forged) })
      .toThrow(InvariantError)
    try {
      ctx.emit(ctx.sidecarUpdates, 'sidecar-updates/status', forged)
    } catch (error) {
      expect((error as InvariantError).message).toMatch(/does not exist on disk/)
    }
  })
})
