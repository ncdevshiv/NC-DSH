/** Startup temp reclaim: exact shapes, retained content, and the launch-request proof. */
import { existsSync } from 'node:fs'
import { chmod, mkdir, mkdtemp, rm, symlink, utimes, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterAll, expect, it } from 'vitest'
import type { TempSweepReport, TempWarnFn } from '../src/temp-cleanup.ts'
import { sweepStaleProcessTemps } from '../src/temp-cleanup.ts'

const SPILL = 'dsh-subprocess-ab12cd'
const LAUNCH = 'dsh-subprocess-launch-ef34gh'
const AGED = new Date(Date.now() - 16 * 60_000)

const bases: string[] = []

/** One scan root holding `name`, its entries, and the age its tree was last touched. */
async function fixture(name: string, entries: Record<string, string>, aged: boolean): Promise<{ base: string; path: string }> {
  const base = await mkdtemp(join(tmpdir(), 'dsh-subprocess-sweep-'))
  bases.push(base)
  const path = join(base, name)
  await mkdir(path, { recursive: true, mode: 0o700 })
  for (const [file, content] of Object.entries(entries)) await writeFile(join(path, file), content, 'utf8')
  const stamp = aged ? AGED : new Date()
  await utimes(path, stamp, stamp)
  for (const file of Object.keys(entries)) await utimes(join(path, file), stamp, stamp)
  return { base, path }
}

async function run(base: string, warn: TempWarnFn, keep?: () => string | undefined): Promise<TempSweepReport> {
  return await sweepStaleProcessTemps(keep === undefined ? { warn, base } : { warn, base, keep })
}

afterAll(async () => {
  for (const base of bases) await rm(base, { recursive: true, force: true })
})

it('retires an aged empty spill tree and keeps one holding output or one still in use', async () => {
  const aged = await fixture(SPILL, {}, true)
  const retired = await run(aged.base, () => {})
  expect(retired.spillDirs).toEqual([aged.path])
  expect(existsSync(aged.path)).toBe(false)

  // A completed spill file is a retained recovery artifact, and the running
  // process's own directory is its next spill's target.
  const kept = await fixture(SPILL, { 'dsh-subprocess-1-out.log': 'output' }, true)
  expect(await run(kept.base, () => {}).then(report => report.spillDirs)).toEqual([])
  expect(existsSync(join(kept.path, 'dsh-subprocess-1-out.log'))).toBe(true)

  const guarded = await fixture(SPILL, {}, true)
  expect(await run(guarded.base, () => {}, () => guarded.path)).toMatchObject({ spillDirs: [] })
  expect(existsSync(guarded.path)).toBe(true)

  // A tree still being worked on belongs to a live provider, however it looks.
  const young = await fixture(SPILL, {}, false)
  expect(await run(young.base, () => {}).then(report => report.spillDirs)).toEqual([])
  expect(existsSync(young.path)).toBe(true)
})

it('retires only an abandoned launch tree, which is the one still holding its request', async () => {
  const abandoned = await fixture(LAUNCH, { 'launch-request.json': '{"cwd":"/tmp","env":{}}' }, true)
  const report = await run(abandoned.base, () => {})
  expect(report.launchDirs).toEqual([abandoned.path])
  expect(existsSync(abandoned.path)).toBe(false)

  // The bootstrap consumes the request as it starts, so a tree without one
  // belongs to a launch that ran or is running, and its residue is not ours.
  const settled = await fixture(LAUNCH, { 'startup-error.json': '{}' }, true)
  expect(await run(settled.base, () => {}).then(r => r.launchDirs)).toEqual([])
  const consumed = await fixture(LAUNCH, {}, true)
  expect(await run(consumed.base, () => {}).then(r => r.launchDirs)).toEqual([])

  // An entry the launch protocol never writes means the tree is not ours to clear.
  const foreign = await fixture(LAUNCH, { 'launch-request.json': '{}', 'note.txt': 'mine' }, true)
  expect(await run(foreign.base, () => {}).then(r => r.launchDirs)).toEqual([])
  expect(existsSync(join(foreign.path, 'note.txt'))).toBe(true)

  const young = await fixture(LAUNCH, { 'launch-request.json': '{}' }, false)
  expect(await run(young.base, () => {}).then(r => r.launchDirs)).toEqual([])
})

it('scans only the directory shapes this provider creates', async () => {
  const base = await mkdtemp(join(tmpdir(), 'dsh-subprocess-sweep-'))
  bases.push(base)
  const warnings: string[] = []
  for (const name of [
    'dsh-subprocess-testfixture',
    'dsh-subprocess-ab12c',
    'dsh-subprocess-launch-abcdefg',
    'dsh-subprocess-ab12cd-extra',
    'not-mine',
  ]) {
    const path = join(base, name)
    await mkdir(path, { recursive: true })
    await utimes(path, AGED, AGED)
  }
  expect(await run(base, (message) => { warnings.push(message) })).toEqual({ spillDirs: [], launchDirs: [] })
  expect(warnings).toEqual([])
  expect(existsSync(join(base, 'dsh-subprocess-testfixture'))).toBe(true)
})

it('reports an unreadable scan root and returns without touching anything', async () => {
  const base = await mkdtemp(join(tmpdir(), 'dsh-subprocess-sweep-'))
  bases.push(base)
  const file = join(base, 'plain.txt')
  await writeFile(file, 'x', 'utf8')
  const warnings: string[] = []
  expect(await run(file, (message) => { warnings.push(message) })).toEqual({ spillDirs: [], launchDirs: [] })
  expect(warnings[0]).toContain('subprocess-local: failed to scan')
})

it('skips a tree another local account could replace', async () => {
  const world = await fixture(SPILL, {}, true)
  await chmod(world.path, 0o777)
  const warnings: string[] = []
  const report = await run(world.base, (message) => { warnings.push(message) })
  if (process.platform === 'win32') {
    // POSIX ownership and mode bits have no Windows equivalent, so the trust
    // check admits the directory and only the age rule applies here.
    expect(report.spillDirs).toEqual([world.path])
  } else {
    expect(report.spillDirs).toEqual([])
    expect(warnings.join()).toContain('subprocess-local: skipped unusable temp directory')
    expect(existsSync(world.path)).toBe(true)
  }
})

it.runIf(process.platform !== 'win32')('never follows a link carrying one of these shapes', async () => {
  const base = await mkdtemp(join(tmpdir(), 'dsh-subprocess-sweep-'))
  bases.push(base)
  const target = join(base, 'outside')
  await mkdir(target)
  await writeFile(join(target, 'keep.txt'), 'foreign', 'utf8')
  await symlink(target, join(base, LAUNCH))
  const warnings: string[] = []
  expect(await run(base, (message) => { warnings.push(message) }).then(r => r.launchDirs)).toEqual([])
  expect(warnings.join()).toContain('skipped unusable temp directory')
  expect(existsSync(join(target, 'keep.txt'))).toBe(true)
})
