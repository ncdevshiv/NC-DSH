import { existsSync } from 'node:fs'
import { describe, expect, it } from 'vitest'
import * as yaml from 'js-yaml'
import { entryListSchema } from '@deepseek-ai/cordis-plugin-include'
import { composeEntries } from '@deepseek-ai/dsh-app-boot'
import { readFile } from 'node:fs/promises'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { resolve } from 'node:path'
import { createRequire } from 'node:module'

const require = createRequire(import.meta.url)
const root = fileURLToPath(new URL('..', import.meta.url))

describe('dsh-base retrieval composition', () => {
  it('mounts the zvec provider, model tool, and session index on by default', async () => {
    const parsed = yaml.load(await readFile(resolve(root, 'cordis.patch.yml'), 'utf8'), { schema: entryListSchema })
    if (!Array.isArray(parsed)) throw new TypeError('base patch must parse to a patch list')
    const entries = composeEntries([parsed as never])
    const retrieval = entries.find(entry => entry.id === 'retrieval-zvec')
    expect(retrieval).toMatchObject({
      id: 'retrieval-zvec',
      name: '@deepseek-ai/dsh-retrieval-zvec',
      config: { embedding: false },
    })
    expect(retrieval?.disabled).toBeUndefined()
    expect(entries.find(entry => entry.id === 'tool-retrieval')).toMatchObject({
      name: '@deepseek-ai/dsh-tool-retrieval',
    })
    expect(entries.find(entry => entry.id === 'retrieval-session-index')).toMatchObject({
      name: '@deepseek-ai/dsh-retrieval-session-index',
    })
    for (const [id, name] of [
      ['retrieval-zvec', '@deepseek-ai/dsh-retrieval-zvec'],
      ['tool-retrieval', '@deepseek-ai/dsh-tool-retrieval'],
      ['retrieval-session-index', '@deepseek-ai/dsh-retrieval-session-index'],
    ] as const) {
      const manifest = JSON.parse(await readFile(resolve(root, 'package.json'), 'utf8')) as { dependencies?: Record<string, string> }
      expect(manifest.dependencies).toHaveProperty(name)
      expect(existsSync(require.resolve(`${name}/package.json`))).toBe(true)
      expect(entries.find(entry => entry.id === id)?.name).toBe(name)
    }
  })

  it('keeps the provider package importable from the built bundle root', () => {
    expect(pathToFileURL(resolve(root, 'src', 'index.ts')).href).toContain('base')
  })
})
