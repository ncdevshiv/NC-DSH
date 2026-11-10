import { mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterEach, describe, expect, it } from 'vitest'
import { runPlatform } from '../src/platform.ts'
import type { RunningPlatform } from '../src/platform.ts'

const platforms: RunningPlatform[] = []
const directories: string[] = []

afterEach(async () => {
  while (platforms.length > 0) {
    const platform = platforms.pop()
    if (platform !== undefined) await platform.stop()
  }
  while (directories.length > 0) {
    const directory = directories.pop()
    if (directory !== undefined) rmSync(directory, { recursive: true, force: true })
  }
})

describe('ncag platform entry point', () => {
  it('starts with no optional dashboard settings and stops cleanly', async () => {
    const root = mkdtempSync(join(tmpdir(), 'ncag-platform-'))
    directories.push(root)
    const platform = await runPlatform({
      NCAG_DATABASE_PATH: join(root, 'platform.db'),
      NCAG_WORKSPACE_ROOT: join(root, 'workspaces'),
      NCAG_MODEL_PRICING: JSON.stringify([{ provider: 'p', model: 'm', inputPerMillion: 1, outputPerMillion: 2 }]),
    }, null, null)
    platforms.push(platform)
    expect(platform.url.startsWith('http://127.0.0.1:')).toBe(true)
    // An unauthenticated snapshot read is allowed only because no token was configured.
    const snapshot = await fetch(platform.url.replace(/\/$/, '') + '/api/snapshot')
    expect(snapshot.status).toBe(200)
    expect(platform.orchestrator.repositories.projects.list()).toEqual([])
  })
})
