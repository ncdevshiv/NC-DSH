/**
 * Vocabulary for the nc-tools filesystem provider: the plugin config and the
 * nc-tools result shapes the provider reads.
 *
 * @module @deepseek-ai/dsh-fs-mcp/types
 */

import type { FsErrorCode } from '@deepseek-ai/dsh-fs'

/**
 * Plugin config for the nc-tools filesystem provider.
 *
 * The provider drives one `nc-tools-mcp` server over MCP stdio. The server
 * is the filesystem AUTHORITY: every read, write, edit and listing goes
 * through it, so the harness's filesystem tools get nc-tools' typed
 * semantics — occurrence-exact edits, atomic writes, content-hash freshness
 * guards — instead of the local provider's own.
 */
export interface NcToolsConfig {
  /** The `nc-tools-mcp` executable to run. */
  command: string
  /** Arguments for the executable. The workspace root is appended. */
  args: string[]
  /** The workspace root the server serves. Relative tool paths resolve here. */
  workspaceRoot: string
  /** How long one MCP tool call may take, in milliseconds. */
  toolCallTimeoutMs: number
  /** How often `watch()` polls the server for a change, in milliseconds. */
  watchPollMs: number
}

/** The `fs.stat` result: presence, kind, size and mtime. */
export interface NcStatResult {
  exists: boolean
  path: string
  type?: string | null
  size?: number | null
  mtimeMs?: number | null
}

/** The `fs.hash` result: the content digest of one file. */
export interface NcHashResult {
  sha256?: string | null
  bytes?: number | null
  algo?: string | null
  match?: boolean | null
}

/** One window of a byte-range read. */
export interface NcReadRangeResult {
  content: string
  byteOffset: number
  byteLength: number
  nextByteOffset: number
  eof: boolean
  fileSize: number
  startLine: number
  lines: number
}

/** One window of a binary-safe byte-range read. */
export interface NcReadBytesResult {
  path: string
  base64: string
  byteOffset: number
  byteLength: number
  nextByteOffset: number
  fileSize: number
  eof: boolean
}

/** One entry of a directory listing. */
export interface NcListEntry {
  name: string
  path: string
  type: string
  size?: number | null
}

/** The `fs.list` result. */
export interface NcListResult {
  entries: NcListEntry[]
  total: number
  truncated: boolean
}

/** The `fs.write` result. */
export interface NcWriteResult {
  path: string
  bytes: number
  created: boolean
  overwritten: boolean
  previousHash?: string | null
}

/** The `patch.apply` result. */
export interface NcPatchResult {
  path: string
  bytes: number
  previousHash: string
  sha256: string
  applied: Array<{ index: number; replacements: number }>
  warnings: unknown[]
}

/** The `patch.check` result: what a planned edit would do, without doing it. */
export interface NcPatchCheckResult {
  allValid: boolean
  edits: Array<{ index: number; valid: boolean; occurrences: number }>
}

/** The structured error nc-tools reports for a failed call. */
export interface NcToolError {
  code: string
  message: string
  hint?: unknown
}

/** The nc-tools error code → harness {@link FsErrorCode} mapping. */
export type NcErrorCodeMap = Readonly<Record<string, FsErrorCode>>
