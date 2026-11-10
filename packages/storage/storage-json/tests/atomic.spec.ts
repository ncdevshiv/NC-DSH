import { existsSync } from 'node:fs'
import { mkdir, mkdtemp, readFile, rm, symlink, utimes, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterAll, expect, it } from 'vitest'
import { JsonStorageBackend } from '../src/index.ts'
import { sweepStalePublishTemps } from '../src/atomic.ts'

const roots: string[] = []

async function freshRoot(): Promise<string> {
  const root = await mkdtemp(join(tmpdir(), 'dsh-storage-json-atomic-'))
  roots.push(root)
  return root
}

afterAll(async () => {
  for (const root of roots) await rm(root, { recursive: true, force: true })
})

/** A scratch name the publish protocol produces, and one it never produces. */
const SCRATCH = '.11111111-2222-3333-4444-555555555555.tmp'
const OTHER_SCRATCH = '.aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee.tmp'
const NOT_SCRATCH = '.settings.json.swp'
const AGED = new Date(Date.now() - 120_000)
/** Older than the shipped sweep policy, which a unit open applies by default. */
const RETIRED_ON_OPEN = new Date(Date.now() - 16 * 60_000)

it('retires stale scratch in every subtree and keeps live scratch, records, and foreign files', async () => {
  const root = await freshRoot()
  await mkdir(join(root, 'shape/t'), { recursive: true })
  const stale = [join(root, SCRATCH), join(root, 'shape/t', OTHER_SCRATCH)]
  const live = join(root, 'shape', SCRATCH)
  const keep = join(root, 'shape', NOT_SCRATCH)
  const record = join(root, 'shape.json')
  for (const path of [...stale, live, keep, record]) await writeFile(path, 'residue', 'utf8')
  for (const path of stale) await utimes(path, AGED, AGED)

  expect(await sweepStalePublishTemps(root, 60_000).then(paths => paths.sort())).toEqual([...stale].sort())
  for (const path of stale) expect(existsSync(path)).toBe(false)
  // A live publish owns its scratch file until the rename, and only the
  // publish protocol's own name shape is ever a candidate.
  for (const path of [live, keep, record]) {
    expect(existsSync(path)).toBe(true)
    expect(await readFile(path, 'utf8')).toBe('residue')
  }
})

it('sweeps an absent tree as an empty result instead of failing the caller', async () => {
  expect(await sweepStalePublishTemps(join(await freshRoot(), 'never-created'))).toEqual([])
})

it.runIf(process.platform !== 'win32')('keeps a scratch entry that cannot be retired', async () => {
  // A scratch file unlinked between the listing and the retire attempt, here
  // made durable as a dangling link so the failure is observed, not raced.
  const root = await freshRoot()
  await symlink(join(root, 'gone'), join(root, SCRATCH))

  expect(await sweepStalePublishTemps(root, 60_000)).toEqual([])
  // The link itself is still there: retiring an entry that cannot be read is
  // not the same as removing it.
  expect(existsSync(join(root, SCRATCH))).toBe(true)
})

it('reclaims the residue an unclean exit left without delaying the unit open', async () => {
  const root = await freshRoot()
  const stale = join(root, SCRATCH)
  await writeFile(stale, 'residue', 'utf8')
  await utimes(stale, RETIRED_ON_OPEN, RETIRED_ON_OPEN)

  const backend = new JsonStorageBackend(root)
  const unit = await backend.kv.open({ name: 'shape', version: 1, tables: ['t'], hasGlobal: false })
  // The reclaim is launched by this open and not awaited by it, so a polluted
  // root cannot stand between a session and its first read; a concurrent write
  // into the same directory is the case the age guard and name filter exist for.
  await unit.putRecord('t', 'kept', { value: 1 })
  await backend.close()

  expect(existsSync(stale)).toBe(false)
  expect(await readFile(join(root, 'shape.json'), 'utf8')).toContain('"kept"')

  const reopened = new JsonStorageBackend(root)
  const restored = await reopened.kv.open({ name: 'shape', version: 1, tables: ['t'], hasGlobal: false })
  await restored.putRecord('t', 'later', { value: 2 })
  await reopened.close()
  expect(await readFile(join(root, 'shape.json'), 'utf8')).toContain('"kept"')
})
