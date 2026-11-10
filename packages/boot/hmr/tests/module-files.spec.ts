/** The file clock ModuleFiles compares module files with. */
import * as fs from 'node:fs'
import { join } from 'node:path'
import { tmpdir } from 'node:os'
import { expect, it, vi } from 'vitest'
import { ModuleFiles, sweepStaleClockDirectories } from '../src/module-files.ts'

vi.mock('node:fs', async original => ({ ...await original<typeof import('node:fs')>() }))

it('reads the time from a new marker file and removes it', () => {
  const created = vi.spyOn(fs, 'mkdtempSync')
  try {
    const time = new ModuleFiles().now()
    expect(time).toBeGreaterThan(0)
    const directory = created.mock.results[0]?.value as string
    expect(fs.existsSync(directory)).toBe(false)
  } finally { created.mockRestore() }
})

it('falls back to the system clock when no marker can be created', () => {
  const created = vi.spyOn(fs, 'mkdtempSync').mockImplementationOnce(() => { throw new Error('read-only') })
  try {
    const before = Date.now()
    expect(new ModuleFiles().now()).toBeGreaterThanOrEqual(before)
  } finally { created.mockRestore() }
})

it('leaves residue in place when the temporary root cannot be read', async () => {
  expect(await sweepStaleClockDirectories(join(tmpdir(), 'dsh-hmr-sweep-missing-root'))).toEqual([])
})

it('retires markers past the age limit and keeps young, foreign, and file entries', async () => {
  const root = fs.mkdtempSync(join(tmpdir(), 'dsh-hmr-sweep-'))
  const aged = join(root, 'dsh-hmr-clock-aged')
  const foreign = join(root, 'not-a-clock-marker')
  const fileShaped = join(root, 'dsh-hmr-clock-a-file')
  try {
    fs.mkdirSync(aged)
    fs.mkdirSync(foreign)
    fs.writeFileSync(fileShaped, '')

    // A freshly created marker is not old enough to retire: a running watcher's
    // marker must survive the sweep.
    expect(await sweepStaleClockDirectories(root, 60_000)).toEqual([])
    expect(fs.existsSync(aged)).toBe(true)

    expect(await sweepStaleClockDirectories(root, 0)).toEqual([aged])
    expect(fs.existsSync(aged)).toBe(false)
    expect(fs.existsSync(foreign)).toBe(true)
    expect(fs.existsSync(fileShaped)).toBe(true)
  } finally {
    fs.rmSync(root, { recursive: true, force: true })
  }
})

it.runIf(process.platform !== 'win32' && process.getuid?.() !== 0)('reports nothing removed when the delete itself fails', async () => {
  // A directory this process may read but not unlink from: the marker stays,
  // and the sweep reports it as not retired instead of failing the start.
  const root = fs.mkdtempSync(join(tmpdir(), 'dsh-hmr-sweep-'))
  const marker = join(root, 'dsh-hmr-clock-held')
  fs.mkdirSync(marker)
  fs.chmodSync(root, 0o500)
  try {
    expect(await sweepStaleClockDirectories(root, 0)).toEqual([])
    expect(fs.existsSync(marker)).toBe(true)
  } finally {
    fs.chmodSync(root, 0o700)
    fs.rmSync(root, { recursive: true, force: true })
  }
})
