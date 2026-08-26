/**
 * Durable store file for the notification seam: one JSON document at
 * `<harness home>/notifications/v1/state.json`, replaced atomically on every
 * mutation through a random-suffix exclusive-create temp sibling plus rename,
 * so a reader or crash observes either the complete previous or the complete
 * next document. Parsing validates the whole document at the durable-file
 * boundary; any deviation is corruption and yields an empty store.
 * @module @deepseek-ai/dsh-notifications/persist
 */

import { randomBytes } from 'node:crypto'
import { mkdirSync, readFileSync, renameSync, rmSync, writeFileSync } from 'node:fs'
import { dirname } from 'node:path'
import type { NotificationView } from './types.ts'

/** The only store format version this build reads; anything else is corruption. */
const STORE_VERSION = 1

/** Permission bits for the state file: user-private under the harness home. */
const FILE_MODE = 0o600
/** Permission bits for created parent directories. */
const DIR_MODE = 0o700

/** Whether a filesystem error means absence; every other error surfaces. */
function isENOENT(error: unknown): boolean {
  return (error as NodeJS.ErrnoException | null)?.code === 'ENOENT'
}

/**
 * Replace `filename` with `content` in one synchronous atomic step, creating
 * parent directories. Mirrors `writeFileAtomic` from dsh-atomic-write with
 * sync primitives so seam mutations keep their void signatures and their
 * ordering: the temp sibling is opened with exclusive create (`wx`) at
 * `mode`, then renamed over the target; any failure removes the temp file.
 * @param filename - final path receiving the content.
 * @param content - complete next file content.
 */
export function writeFileAtomicSync(filename: string, content: string): void {
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
 * Read the store text, or `undefined` when the file does not exist yet.
 * @param filename - absolute store path.
 * @returns the document text, or `undefined` for absence.
 */
export function readStoreText(filename: string): string | undefined {
  try {
    return readFileSync(filename, 'utf8')
  } catch (error) {
    if (isENOENT(error)) return undefined
    throw error
  }
}

/** Whether one parsed row carries exactly the storable view fields. */
function isValidRow(row: unknown): row is NotificationView {
  if (typeof row !== 'object' || row === null || Array.isArray(row)) return false
  const record = row as Record<string, unknown>
  if (typeof record['id'] !== 'string' || record['id'].length === 0) return false
  if (typeof record['kind'] !== 'string') return false
  if (typeof record['title'] !== 'string') return false
  if (record['body'] !== undefined && typeof record['body'] !== 'string') return false
  if (record['data'] !== undefined && (typeof record['data'] !== 'object' || record['data'] === null)) return false
  if (typeof record['createdAt'] !== 'string') return false
  if (typeof record['read'] !== 'boolean') return false
  return typeof record['dismissed'] === 'boolean'
}

/**
 * Parse one store document. Every structural deviation — bad JSON, a
 * non-object root, an unknown version, a non-array list, or any invalid row —
 * throws, which callers translate into "start empty" plus one warning, so a
 * corrupt file can never surface partial rows.
 * @param text - the document's text.
 * @returns the validated rows in stored order.
 */
export function parseStore(text: string): NotificationView[] {
  const root: unknown = JSON.parse(text)
  if (typeof root !== 'object' || root === null || Array.isArray(root)) {
    throw new TypeError('store root must be an object')
  }
  const document = root as Record<string, unknown>
  if (document['version'] !== STORE_VERSION) {
    throw new TypeError(`store version must be ${String(STORE_VERSION)}`)
  }
  const rows = document['notifications']
  if (!Array.isArray(rows)) throw new TypeError('store notifications must be an array')
  if (!rows.every(isValidRow)) throw new TypeError('store contains an invalid notification row')
  return structuredClone(rows)
}

/**
 * Render the complete next store document.
 * @param records - every live row in stored order.
 * @returns the document text to persist.
 */
export function renderStore(records: readonly NotificationView[]): string {
  return `${JSON.stringify({ version: STORE_VERSION, notifications: records }, null, 2)}\n`
}
