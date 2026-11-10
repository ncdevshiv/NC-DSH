/**
 * Filesystem provider backed by the nc-tools MCP server.
 *
 * The server is the filesystem AUTHORITY: every read, write, edit and
 * listing goes through it. Swapping this provider in for the local one
 * therefore swaps in nc-tools' semantics — occurrence-exact edits, atomic
 * writes, a content-hash freshness guard — without the harness's tool layer
 * changing at all.
 *
 * @module @deepseek-ai/dsh-fs-mcp
 */

import { createHash } from 'node:crypto'
import { isAbsolute, relative, resolve } from 'node:path'
import { pathToFileURL } from 'node:url'
import { Context, Service } from '@deepseek-ai/cordis'
import z from '@deepseek-ai/schemastery'
import { FileSystem, FsError, FsTargetKey, FsVersion } from '@deepseek-ai/dsh-fs'
import type {
  FsDirEntry,
  FsEditOutcome,
  FsEditRequest,
  FsInfo,
  FsPathInfo,
  FsTarget,
  FsWriteIntent,
  FsWriteOutcome,
} from '@deepseek-ai/dsh-fs'
import { NcToolsClient } from './client.ts'
import { toFsError } from './errors.ts'
import type {
  NcHashResult,
  NcListResult,
  NcPatchCheckResult,
  NcPatchResult,
  NcReadBytesResult,
  NcReadRangeResult,
  NcStatResult,
  NcToolsConfig,
  NcWriteResult,
} from './types.ts'

/** Configuration for the nc-tools filesystem backend. */
export interface Config {
  /** The `nc-tools-mcp` executable to run. */
  command: string
  /** Arguments for the executable. The workspace root is appended. */
  args?: string[]
  /** The workspace root the server serves. Defaults to `process.cwd()`. */
  workspaceRoot?: string
  /** How long one MCP tool call may take, in milliseconds. Defaults to 60 000. */
  toolCallTimeoutMs?: number
  /** How often `watch()` polls the server for a change, in milliseconds. Defaults to 500. */
  watchPollMs?: number
}

type ResolvedConfig = Required<Config>

const DEFAULT_TOOL_CALL_TIMEOUT_MS = 60_000
const DEFAULT_WATCH_POLL_MS = 500
/** The largest window `fs.readRange` will return in one call. */
const READ_WINDOW_BYTES = 1_048_576
/** The largest window `fs.readBytes` will return in one call. */
const READ_BYTES_WINDOW_BYTES = 4_194_304

/**
 * The nc-tools filesystem backend.
 *
 * Every operation is one `tools/call` round trip over MCP stdio, so the
 * provider pays a process hop per primitive. That is the cost of making an
 * external server the authority, and it is the same cost the SSH-backed
 * provider pays.
 */
export class NcToolsFileSystem extends FileSystem {
  static Config: z<Config> = z.object({
    command: z.string().required(),
    args: z.array(String).default([]),
    workspaceRoot: z.string().default(process.cwd()),
    toolCallTimeoutMs: z.number().default(DEFAULT_TOOL_CALL_TIMEOUT_MS),
    watchPollMs: z.number().default(DEFAULT_WATCH_POLL_MS),
  })

  private readonly config: ResolvedConfig
  private client: NcToolsClient | undefined

  constructor(ctx: Context, config: ResolvedConfig) {
    super(ctx)
    this.config = config
  }

  /**
   * Connect to the server before the provider takes traffic, and close it
   * when the provider is disposed.
   *
   * The connect is asynchronous, so it cannot happen in the constructor: a
   * service that answered a call before its server was up would fail the
   * first request with a transport error instead of a clear "not ready".
   */
  async* [Service.init](): AsyncGenerator<() => Promise<void>, void, unknown> {
    this.client = await NcToolsClient.connect(
      this.config.command,
      this.config.args,
      this.config.workspaceRoot,
    )
    yield async () => {
      const client = this.client
      this.client = undefined
      await client?.close()
    }
  }

  override resolve(path: string, opts?: { cwd?: string; signal?: AbortSignal }): Promise<FsTarget> {
    opts?.signal?.throwIfAborted()
    const base = opts?.cwd ?? this.config.workspaceRoot
    const absolute = isAbsolute(path) ? resolve(path) : resolve(base, path)
    return Promise.resolve({ targetKey: FsTargetKey(absolute), displayPath: absolute })
  }

  override processPath(target: FsTarget): string {
    return String(target.targetKey)
  }

  override processPathFromHostPath(hostPath: string): string | undefined {
    // The server serves the harness host's filesystem in the shipped stdio
    // deployment, so a host path under the workspace is the same file. A
    // path outside the workspace is NOT the server's to answer for.
    const within = relative(this.config.workspaceRoot, hostPath)
    if (within === '' || (!within.startsWith('..') && !isAbsolute(within))) {
      return hostPath
    }
    return undefined
  }

  override fileUrl(target: FsTarget): string {
    return pathToFileURL(this.processPath(target)).href
  }

  override contains(parent: FsTarget, child: FsTarget): boolean {
    const path = relative(this.processPath(parent), this.processPath(child))
    return path === '' || (!path.startsWith('..') && path !== '..' && !isAbsolute(path))
  }

  override async stat(target: FsTarget, signal?: AbortSignal): Promise<FsInfo | undefined> {
    const path = this.processPath(target)
    const stat = await this.call<NcStatResult>('fs.stat', { path }, signal)
    if (!stat.exists) return undefined
    const type = stat.type === 'dir' ? 'directory' : stat.type === 'file' ? 'file' : 'other'
    const version = type === 'file'
      // A file's version is its content hash: the same token nc-tools' own
      // write and edit paths report, so a guard minted from one is
      // checkable by the other.
      ? FsVersion(await this.contentHash(path, signal))
      // A directory has no content to hash. Its mtime is the freshness fact
      // the server can report cheaply.
      : FsVersion(`dir:${stat.mtimeMs ?? 0}`)
    return {
      version,
      type,
      ...(typeof stat.size === 'number' ? { size: stat.size } : {}),
    }
  }

  override async lstat(path: string, opts?: { cwd?: string; signal?: AbortSignal }): Promise<FsPathInfo | undefined> {
    const target = await this.resolve(path, opts)
    const info = await this.stat(target, opts?.signal)
    if (info === undefined) return undefined
    return { version: info.version, type: info.type, ...(info.size !== undefined ? { size: info.size } : {}) }
  }

  override async readText(target: FsTarget, signal?: AbortSignal): Promise<string> {
    await this.assertRegularFile(this.processPath(target), signal)
    let text = ''
    for await (const chunk of this.windows(target, signal)) {
      text += chunk
    }
    return text
  }

  override streamText(target: FsTarget, signal?: AbortSignal): Promise<AsyncIterable<string>> {
    const path = this.processPath(target)
    const windows = (async function* (self: NcToolsFileSystem) {
      await self.assertRegularFile(path, signal)
      for await (const chunk of self.windows(target, signal)) {
        yield chunk
      }
    })(this)
    return Promise.resolve(windows)
  }

  override async readBytes(
    target: FsTarget,
    signal: AbortSignal | undefined,
    maxBytes: number,
  ): Promise<Uint8Array> {
    const path = this.processPath(target)
    const stat = await this.call<NcStatResult>('fs.stat', { path }, signal)
    if (!stat.exists) {
      throw new FsError(`No such file: ${path}`, 'FS_NOT_FOUND')
    }
    if (stat.type === 'dir') {
      throw new FsError(`${path} is a directory`, 'FS_NOT_DIRECTORY')
    }
    if (typeof stat.size === 'number' && stat.size > maxBytes) {
      // The seam's bound is on the COMPLETE content: a backend must fail
      // rather than hand back a truncated file that looks complete.
      throw new FsError(`${path} is ${stat.size} bytes, over the ${maxBytes} byte limit`, 'FS_TOO_LARGE')
    }
    const chunks: Uint8Array[] = []
    let total = 0
    for await (const chunk of this.byteWindows(path, maxBytes, signal)) {
      chunks.push(chunk)
      total += chunk.length
    }
    const out = new Uint8Array(total)
    let at = 0
    for (const chunk of chunks) {
      out.set(chunk, at)
      at += chunk.length
    }
    return out
  }

  override async readByteRange(
    target: FsTarget,
    range: { offset: number; length: number },
    signal?: AbortSignal,
  ): Promise<Uint8Array> {
    const path = this.processPath(target)
    await this.assertRegularFile(path, signal)
    const chunks: Uint8Array[] = []
    let total = 0
    for await (const chunk of this.byteWindows(path, range.length, signal, range.offset)) {
      chunks.push(chunk)
      total += chunk.length
    }
    const out = new Uint8Array(total)
    let at = 0
    for (const chunk of chunks) {
      out.set(chunk, at)
      at += chunk.length
    }
    return out
  }

  override async listDir(target: FsTarget, signal?: AbortSignal): Promise<FsDirEntry[]> {
    const path = this.processPath(target)
    const result = await this.call<NcListResult>('fs.list', { path }, signal)
    const entries: FsDirEntry[] = []
    for (const entry of result.entries) {
      const child = await this.resolve(entry.path)
      entries.push({
        name: entry.name,
        /* v8 ignore next 3 -- nc-tools reports a directory entry as 'dir' or
         * 'file', so the seam's third kind ('other') never arrives from it. */
        type: entry.type === 'dir' ? 'directory' : entry.type === 'file' ? 'file' : 'other',
        target: child,
        ...(typeof entry.size === 'number' ? { size: entry.size } : {}),
      })
    }
    return entries
  }

  override async writeText(
    target: FsTarget,
    content: string,
    expected?: FsWriteIntent,
    signal?: AbortSignal,
  ): Promise<FsWriteOutcome> {
    const path = this.processPath(target)
    const before = await this.currentContent(path, signal)
    if (expected !== undefined) {
      if (expected.kind === 'createIfAbsent' && before !== null) {
        throw new FsError(`${path} already exists`, 'FS_NOT_OBSERVED')
      }
      if (expected.kind === 'replaceIfVersion') {
        const current = before === null ? null : await this.contentHash(path, signal)
        /* v8 ignore next 3 -- the guard's satisfied arm is exercised: the
         * guarded write under a matching version succeeds, which is what the
         * `replaces against the current version` case asserts. v8 does not
         * credit the fall-through of a throwing `if` that ends its block. */
        if (this.isStale(current, expected.version)) {
          throw new FsError(`${path} changed since it was read`, 'FS_STALE_VERSION')
        }
      }
    }
    await this.call<NcWriteResult>('fs.write', { path, content }, signal)
    return {
      operation: before === null ? 'create' : 'update',
      // The server wrote `content` verbatim as UTF-8, so the digest of those
      // bytes is the file's new version — the same token the server would
      // report for it.
      version: FsVersion(sha256(content)),
      before: before === null ? null : before,
      after: normalizeLineEndings(content),
    }
  }

  override async editText(
    target: FsTarget,
    edit: FsEditRequest,
    expected?: { version: FsVersion },
    signal?: AbortSignal,
  ): Promise<FsEditOutcome> {
    const path = this.processPath(target)
    const before = await this.currentContent(path, signal)
    if (before === null) {
      throw new FsError(`No such file: ${path}`, 'FS_NOT_FOUND')
    }
    if (expected !== undefined) {
      const current = await this.contentHash(path, signal)
      if (current !== String(expected.version)) {
        throw new FsError(`${path} changed since it was read`, 'FS_STALE_VERSION')
      }
    }
    // The occurrence count decides the edit's shape, and the server counts
    // it: `expectedCount: 1` is what makes an ambiguous match an error
    // rather than a silent first-match replacement.
    const occurrences = await this.countOccurrences(path, edit.oldString, signal)
    if (occurrences === 0) {
      throw new FsError(`The text to replace was not found in ${path}`, 'FS_EDIT_NOT_FOUND')
    }
    if (!edit.replaceAll && occurrences > 1) {
      throw new FsError(
        `The text to replace occurs ${occurrences} times in ${path}; the edit requires exactly one`,
        'FS_AMBIGUOUS_EDIT',
      )
    }
    const applied = await this.call<NcPatchResult>(
      'patch.apply',
      {
        path,
        edits: [{ oldText: edit.oldString, newText: edit.newString, expectedCount: occurrences }],
      },
      signal,
    )
    return {
      version: FsVersion(applied.sha256),
      before: normalizeLineEndings(before),
      after: normalizeLineEndings(applyLiteral(before, edit)),
    }
  }

  override async watch(
    target: FsTarget,
    changed: (error?: Error) => void,
    signal: AbortSignal,
  ): Promise<() => Promise<void>> {
    signal.throwIfAborted()
    const path = this.processPath(target)
    let last = await this.watchFingerprint(path, signal)
    let timer: ReturnType<typeof setTimeout>
    let stopped = false
    // Read through a call: the disposer's assignment is invisible to
    // control-flow narrowing inside this closure, which would otherwise treat a
    // stop landing during a poll as impossible.
    const isStopped = (): boolean => stopped
    const poll = (): void => {
      timer = setTimeout(() => {
        void (async () => {
          /* v8 ignore next 3 -- stopping the watcher clears the pending timer,
           * so a poll only finds `stopped` set when it was already in flight
           * when the caller stopped it. */
          if (isStopped()) return
          try {
            const next = await this.watchFingerprint(path, signal)
            if (next !== last) {
              last = next
              changed()
            }
          } catch (error) {
            /* v8 ignore next 3 -- a tool failure is always an Error, so the
             * non-Error arm is unreachable. */
            changed(error instanceof Error ? error : new Error(String(error)))
          }
          // A caller that stopped the watcher mid-poll holds no timer, so the
          // disposer's clearTimeout has already run: re-arming here would leave
          // one poll nobody can cancel, delivering past close until it self-terminates.
          /* v8 ignore next -- the same in-flight stop this poll cannot schedule. */
          if (isStopped()) return
          poll()
        })()
      }, this.config.watchPollMs)
    }
    poll()
    return () => {
      stopped = true
      clearTimeout(timer)
      return Promise.resolve()
    }
  }

  /**
   * Call one nc-tools tool and return its result.
   *
   * @param tool - The nc-tools tool name.
   * @param args - The tool arguments.
   * @param signal - Cancels the call.
   * @returns The tool's parsed result.
   */
  private async call<T>(tool: string, args: Record<string, unknown>, signal?: AbortSignal): Promise<T> {
    const client = this.client
    if (client === undefined) {
      throw new FsError('the nc-tools server is not connected', 'FS_IO_ERROR')
    }
    const outcome = await client.call(tool, args, this.config.toolCallTimeoutMs, signal)
    if (outcome.ok) return outcome.value as T
    throw toFsError(outcome.error)
  }

  /**
   * Assert that a path is a readable regular file.
   *
   * The seam reports a directory as `FS_NOT_DIRECTORY` rather than letting
   * the read fail with whatever the platform says. A directory read is a
   * different mistake from a missing file, and the caller can act on it.
   *
   * @param path - The absolute path.
   * @param signal - Cancels the call.
   */
  private async assertRegularFile(path: string, signal?: AbortSignal): Promise<void> {
    const stat = await this.call<NcStatResult>('fs.stat', { path }, signal)
    if (!stat.exists) {
      throw new FsError(`No such file: ${path}`, 'FS_NOT_FOUND')
    }
    if (stat.type === 'dir') {
      throw new FsError(`${path} is a directory`, 'FS_NOT_DIRECTORY')
    }
  }

  /**
   * Whether a guard's version no longer matches the file's content.
   *
   * A target that is gone has no current version, so it can never satisfy a
   * guard — that is a stale target, not a fresh one.
   *
   * @param current - The file's current content digest, or null when it has none.
   * @param expected - The version the caller observed.
   * @returns True when the file changed since the caller observed it.
   */
  private isStale(current: string | null, expected: FsVersion): boolean {
    return current !== String(expected)
  }

  /**
   * The file's current text, or null when it does not exist.
   *
   * @param path - The absolute path.
   * @param signal - Cancels the read.
   * @returns The file's text, or null.
   */
  private async currentContent(path: string, signal?: AbortSignal): Promise<string | null> {
    const stat = await this.call<NcStatResult>('fs.stat', { path }, signal)
    if (!stat.exists) return null
    if (stat.type === 'dir') {
      throw new FsError(`${path} is a directory`, 'FS_NOT_DIRECTORY')
    }
    let text = ''
    for await (const chunk of this.windows({ targetKey: FsTargetKey(path), displayPath: path }, signal)) {
      text += chunk
    }
    return text
  }

  /**
   * The file's content digest.
   *
   * @param path - The absolute path.
   * @param signal - Cancels the call.
   * @returns The sha256 hex digest.
   * @throws When the file has no digest — it is gone, or the server could not
   *   read it. A version token that silently became a placeholder would let
   *   a stale write through the guard.
   */
  private async contentHash(path: string, signal?: AbortSignal): Promise<string> {
    const hash = await this.call<NcHashResult>('fs.hash', { path }, signal)
    if (hash.sha256 === undefined || hash.sha256 === null) {
      throw new FsError(`cannot read the content of ${path}`, 'FS_NOT_FOUND')
    }
    return hash.sha256
  }

  /**
   * The text windows of a file, in order.
   *
   * @param target - The file to read.
   * @param signal - Cancels the read.
   * @returns An async iterable of decoded text windows.
   */
  private async *windows(target: FsTarget, signal?: AbortSignal): AsyncGenerator<string> {
    const path = this.processPath(target)
    let offset = 0
    for (;;) {
      signal?.throwIfAborted()
      const window = await this.call<NcReadRangeResult>(
        'fs.readRange',
        { path, byteOffset: offset, maxBytes: READ_WINDOW_BYTES },
        signal,
      )
      if (window.content.length > 0) yield window.content
      /* v8 ignore next 4 -- nc-tools advances nextByteOffset by the bytes it
       * read, so a window that does not advance cannot come from it. The
       * guard is what stops a misbehaving server from looping forever. */
      if (window.eof || window.nextByteOffset <= offset) return
      offset = window.nextByteOffset
    }
  }

  /**
   * The byte windows of a file, in order.
   *
   * @param path - The absolute path.
   * @param limit - The largest number of bytes to return in total.
   * @param signal - Cancels the read.
   * @param from - The first byte to return.
   * @returns An async iterable of byte windows.
   */
  private async *byteWindows(
    path: string,
    limit: number,
    signal?: AbortSignal,
    from = 0,
  ): AsyncGenerator<Uint8Array> {
    let offset = from
    let total = 0
    for (;;) {
      signal?.throwIfAborted()
      if (total >= limit) return
      const window = await this.call<NcReadBytesResult>(
        'fs.readBytes',
        { path, byteOffset: offset, maxBytes: Math.min(READ_BYTES_WINDOW_BYTES, limit - total) },
        signal,
      )
      if (window.base64.length > 0) {
        const bytes = Buffer.from(window.base64, 'base64')
        yield bytes
        total += bytes.length
      }
      if (window.eof || window.nextByteOffset <= offset) return
      offset = window.nextByteOffset
    }
  }

  /**
   * How many times a literal appears in a file.
   *
   * @param path - The absolute path.
   * @param text - The literal to count.
   * @param signal - Cancels the call.
   * @returns The occurrence count.
   */
  private async countOccurrences(path: string, text: string, signal?: AbortSignal): Promise<number> {
    const check = await this.call<NcPatchCheckResult>(
      'patch.check',
      { path, edits: [{ oldText: text, newText: text }] },
      signal,
    )
    /* v8 ignore next 3 -- the check is asked about exactly one edit, so the
     * server always answers about that edit. */
    return check.edits[0]?.occurrences ?? 0
  }

  /**
   * A cheap change fingerprint for the watcher.
   *
   * @param path - The absolute path.
   * @param signal - Cancels the call.
   * @returns The fingerprint.
   */
  private async watchFingerprint(path: string, signal?: AbortSignal): Promise<string> {
    const stat = await this.call<NcStatResult>('fs.stat', { path }, signal)
    return `${stat.exists}:${stat.mtimeMs ?? 0}:${stat.size ?? 0}`
  }
}

/**
 * Apply a literal replacement to a text, the way the server applies it.
 *
 * @param text - The original text.
 * @param edit - The replacement request.
 * @returns The edited text.
 */
function applyLiteral(text: string, edit: FsEditRequest): string {
  return edit.replaceAll
    ? text.split(edit.oldString).join(edit.newString)
    : text.replace(edit.oldString, edit.newString)
}

/**
 * The sha256 hex digest of a string's UTF-8 bytes.
 *
 * @param text - The text to digest.
 * @returns The hex digest.
 */
function sha256(text: string): string {
  return createHash('sha256').update(Buffer.from(text, 'utf8')).digest('hex')
}

/**
 * Normalize line endings to LF, the diff basis the seam shares.
 *
 * @param text - The text to normalize.
 * @returns The normalized text.
 */
function normalizeLineEndings(text: string): string {
  return text.replace(/\r\n/g, '\n')
}

export default NcToolsFileSystem
export type { Config as NcToolsFileSystemConfig }
export type {
  NcHashResult,
  NcListResult,
  NcPatchCheckResult,
  NcPatchResult,
  NcReadBytesResult,
  NcReadRangeResult,
  NcStatResult,
  NcToolsConfig,
  NcWriteResult,
}
