import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { pathToFileURL } from 'node:url'
import { afterEach, describe, expect, it } from 'vitest'
import { Context } from '@deepseek-ai/cordis'
import Include from '@deepseek-ai/cordis-plugin-include'
import Loader from '@deepseek-ai/cordis-plugin-loader'
import NotificationsService from '@deepseek-ai/dsh-notifications'
import SidecarUpdatesService from '../src/index.ts'
import { startFakeGithub } from './fake-github.ts'

let root: string | undefined
const contexts: Context[] = []
const servers: Array<{ close(): Promise<void> }> = []

afterEach(async () => {
  await Promise.all(contexts.splice(0).map(ctx => ctx.fiber.dispose()))
  await Promise.all(servers.splice(0).map(server => server.close()))
  if (root !== undefined) await rm(root, { recursive: true, force: true })
  root = undefined
})

/** Boot one cordis.yml composition through the real Loader. */
async function loadComposition(configPath: string): Promise<Context> {
  const ctx = new Context()
  contexts.push(ctx)
  ctx.baseUrl = pathToFileURL(root as string).href + '/'
  await ctx.plugin(Loader)
  ctx.loader.builtins.include = Include
  const modules = new Map<string, unknown>([
    ['@deepseek-ai/dsh-notifications', NotificationsService],
    ['@deepseek-ai/dsh-sidecar-updates', SidecarUpdatesService],
  ])
  ctx.loader.internal = {
    version: 'v2',
    async import(specifier: string) {
      if (!modules.has(specifier)) throw new Error(`unexpected Loader import: ${specifier}`)
      return modules.get(specifier)
    },
  } as unknown as NonNullable<typeof ctx.loader.internal>
  await ctx.loader.create({
    name: 'cordis:include',
    config: { path: pathToFileURL(configPath).href },
  })
  await ctx.loader.await()
  const unloaded = [...ctx.loader.entries()]
    .filter(entry => entry.fiber === undefined && !entry.disabled)
    .map(entry => entry.options.name)
  expect(unloaded).toEqual([])
  return ctx
}

describe('sidecar updates through a real Loader composition', () => {
  it('publishes check and install outcomes into the notification seam', async () => {
    root = await mkdtemp(join(tmpdir(), 'dsh-sidecar-updates-loader-'))
    const server = await startFakeGithub({ tag: 'v2.0.1' })
    servers.push(server)
    const configPath = join(root, 'cordis.yml')
    await writeFile(configPath, [
      "- name: '@deepseek-ai/dsh-notifications'",
      '  config:',
      `    dshHome: ${JSON.stringify(root)}`,
      "- name: '@deepseek-ai/dsh-sidecar-updates'",
      '  config:',
      '    repo: owner/repo',
      `    apiBase: ${JSON.stringify(server.url)}`,
      `    installDir: ${JSON.stringify(join(root, 'core-deps', 'ai-sdk'))}`,
      '    checkOnStart: false',
      '    autoInstallOnFirstRun: false',
      '',
    ].join('\n'))

    const ctx = await loadComposition(configPath)

    // Check: the pipeline publishes its actionable notice into the seam.
    const status = await ctx.sidecarUpdates.checkNow()
    expect(status.updateAvailable).toBe(true)
    const updateNotice = ctx.notifications.list().find(entry => entry.id === 'sdk-update:v2.0.1')
    expect(updateNotice?.kind).toBe('sdk-update')
    expect(updateNotice?.body).toBe('Installed none → available v2.0.1')

    // Install: the pointer commits and the installed notice replaces the ask.
    const result = await ctx.sidecarUpdates.install()
    expect(result.restartRequired).toBe(true)
    const pointerPath = join(root, 'core-deps', 'ai-sdk', 'current.json')
    expect((await readPointerSafe(pointerPath))?.tag).toBe('v2.0.1')
    const installedNotice = ctx.notifications.list()
      .find(entry => entry.id === 'sdk-update-installed:v2.0.1')
    expect(installedNotice?.title).toBe('AI SDK v2.0.1 installed')
    expect(ctx.notifications.list().some(entry => entry.id.startsWith('sdk-update:'))).toBe(false)
  })
})

/** Read the pointer document for assertions; null when absent or corrupt. */
async function readPointerSafe(filename: string): Promise<{ tag: string } | null> {
  let text: string
  try {
    text = await readFile(filename, 'utf8')
  } catch {
    return null
  }
  try {
    return JSON.parse(text) as { tag: string }
  } catch {
    return null
  }
}
