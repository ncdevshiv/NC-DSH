/**
 * Durable documents of the sidecar-update pipeline: the `current.json`
 * pointer, the `ignored.json` ignore list, and the staged download bytes.
 * Documents are replaced atomically through a random-suffix exclusive-create
 * temp sibling plus rename, so a crash leaves either the complete previous or
 * the complete next document, never a partial pointer. Parsing validates the
 * whole document at the durable-file boundary; a corrupt pointer or ignore
 * list reads as absent rather than guessing at its content.
 * @module @deepseek-ai/dsh-sidecar-updates/persist
 */

import { randomBytes } from 'node:crypto'
import { mkdirSync, readFileSync, renameSync, rmSync, writeFileSync } from 'node:fs'
import { dirname } from 'node:path'
import type { InstalledEntry } from './types.ts'

/** Permission bits for created files: user-private under the install directory. */
const FILE_MODE = 0o600
/** Permission bits for created parent directories. */
const DIR_MODE = 0o700

/** Whether a filesystem error means absence; every other error surfaces. */
function isENOENT(error: unknown): boolean {
  return (error as NodeJS.ErrnoException | null)?.code === 'ENOENT'
}

/**
 * Replace `filename` with `content` in one synchronous atomic step, creating
 * parent directories. Sync twin of dsh-atomic-write's `writeFileAtomic`: the
 * temp sibling is opened with exclusive create (`wx`) and renamed over the
 * target; any failure removes the temp file first.
 * @param filename - final path receiving the content.
 * @param content - complete next file content (text or binary).
 */
export function writeAtomicSync(filename: string, content: string | Uint8Array): void {
  mkdirSync(dirname(filename), { recursive: true, mode: DIR_MODE })
  const temp = `${filename}.${randomBytes(6).toString('hex')}.tmp`
  try {
    writeFileSync(temp, content, { mode: FILE_MODE, flag: 'wx' })
    renameSync(temp, filename)
  } catch (error) {
    rmSync(temp, { force: true })
    throw error
  }
}

/**
 * Read a document's text, or `undefined` when absent; other errors surface.
 * @param filename - absolute document path.
 * @returns the document text, or `undefined` for absence.
 */
export function readFileOrAbsent(filename: string): string | undefined {
  try {
    return readFileSync(filename, 'utf8')
  } catch (error) {
    if (isENOENT(error)) return undefined
    throw error
  }
}

/** Whether one parsed value carries exactly the pointer fields with valid types. */
function isValidPointer(value: unknown): value is InstalledEntry {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) return false
  const record = value as Record<string, unknown>
  return typeof record['tag'] === 'string' && record['tag'].length > 0
    && typeof record['asset'] === 'string' && record['asset'].length > 0
    && typeof record['sha256'] === 'string' && record['sha256'].length > 0
    && typeof record['installedAt'] === 'string'
    && typeof record['exePath'] === 'string' && record['exePath'].length > 0
}

/**
 * Read the installed-release pointer, or `null` when the file is absent or
 * corrupt. A corrupt pointer warns and behaves as "nothing installed" so a
 * damaged document cannot block a fresh install.
 * @param filename - absolute pointer path.
 * @returns the parsed pointer entry, or `null`.
 */
export function readPointer(filename: string): InstalledEntry | null {
  const text = readFileOrAbsent(filename)
  if (text === undefined) return null
  let parsed: unknown
  try {
    parsed = JSON.parse(text)
  } catch {
    // Unparsable JSON is corruption like any other; nothing else can reach
    // this arm, and treating it as absent keeps installs unblocked.
    return null
  }
  if (!isValidPointer(parsed)) return null
  return parsed
}

/**
 * Atomically repoint the install directory at one release.
 * @param filename - absolute pointer path.
 * @param entry - the complete next pointer entry.
 */
export function writePointer(filename: string, entry: InstalledEntry): void {
  writeAtomicSync(filename, `${JSON.stringify(entry, null, 2)}\n`)
}

/**
 * Read the persisted ignore list, or an empty set when the file is absent or
 * corrupt. The list only ever suppresses notifications and auto-updates, so
 * corruption degrades to "ignore nothing" without blocking installs.
 * @param filename - absolute ignore-list path.
 * @returns the persisted tags.
 */
export function readIgnoredTags(filename: string): readonly string[] {
  const text = readFileOrAbsent(filename)
  if (text === undefined) return []
  let parsed: unknown
  try {
    parsed = JSON.parse(text)
  } catch {
    // Unparsable JSON degrades to ignoring nothing; suppression state is
    // recoverable through ignore() while installs stay unblocked.
    return []
  }
  if (!Array.isArray(parsed) || !parsed.every(row => typeof row === 'string')) return []
  return parsed
}

/**
 * Atomically replace the ignore list.
 * @param filename - absolute ignore-list path.
 * @param tags - the complete next tag set.
 */
export function writeIgnoredTags(filename: string, tags: readonly string[]): void {
  writeAtomicSync(filename, `${JSON.stringify(tags, null, 2)}\n`)
}
