/**
 * Rewind-time workspace restore: inverse-replays the discarded turn slice of a
 * session log using the full-text restore basis the `write`/`edit` tools attach
 * to `tool/result` meta. A rewind (session.fork with `beforeSeq`) calls this
 * service before the child can run; mutations without a basis (`bash`, `pwsh`,
 * `terminal`, `str_replace_editor`, size-capped writes) are counted and
 * reported, never silently dropped and never guessed at.
 * @module @deepseek-ai/dsh-turn-restore
 */

import { readFile, rm, writeFile } from 'node:fs/promises'
import path from 'node:path'
import type { Context } from '@deepseek-ai/cordis'
import type { SessionEvent } from '@deepseek-ai/dsh-session'
import { basisFromMeta } from '@deepseek-ai/dsh-tool-fs'
import type { FsDiffBasis } from '@deepseek-ai/dsh-tool-fs'

/** Tool names whose side effects never record a restore basis. */
const SHELL_TOOL_NAMES: ReadonlySet<string> = new Set(['bash', 'pwsh', 'terminal'])
/** File tools that should carry a basis but whose present result has none. */
export const BASISLESS_FS_TOOLS: ReadonlySet<string> = new Set([
  'write',
  'edit',
  'str_replace_editor',
])

/** Counts and facts of one restore pass, safe to forward to a client notice. */
export interface TurnRestoreReport {
  /** Files rewritten (or created-then-deleted) to their pre-turn state. */
  restored: number
  /** Display paths skipped because the disk content no longer matches the basis. */
  conflicts: string[]
  /** Mutations whose before-text was not recorded (size cap, basis-less tool). */
  notRestorable: { count: number; toolNames: string[] }
  /** Shell invocations in the discarded slice; their side effects cannot be reverted. */
  shell: { count: number; names: string[] }
  /** Why the workspace was left untouched, when the restore could not run at all. */
  skipped?: 'source-running' | 'no-cwd'
}

/** One restore request: the exact discarded log slice plus its workspace root. */
export interface TurnRestoreInput {
  /** Discarded events in log order (seq `cut` onward), never over the retained prefix. */
  events: readonly SessionEvent[]
  /** Session workspace root the display paths resolve against. */
  cwd: string
}

/** One restore-planned mutation, in application order (reverse log order). */
interface RestorePlan {
  basis: FsDiffBasis
  abs: string
}

/**
 * Rewind-time workspace restore service, called by the host's `session.fork`
 * before it publishes a `beforeSeq` child.
 */
export class TurnRestore {
  /**
   * Inverse-replay `input.events` and return the outcome. The pass is
   * best-effort and observationally safe: it only rewrites a file whose current
   * content still equals the basis's `after`, so an intervening user edit makes
   * that entry a reported conflict instead of a clobber.
   * @param input - discarded events and the workspace root.
   * @returns the per-file outcome counts and skips.
   */
  async restore(input: TurnRestoreInput): Promise<TurnRestoreReport> {
    const report: TurnRestoreReport = {
      restored: 0,
      conflicts: [],
      notRestorable: { count: 0, toolNames: [] },
      shell: { count: 0, names: [] },
    }
    const toolNames = new Set<string>()
    const shellNames = new Set<string>()
    const plans: RestorePlan[] = []
    // Forward pass: pair call ids with tool names (a tool/result has no name of
    // its own) while collecting the plans, then apply newest-first so repeated
    // edits to one file step back through every intermediate state.
    const nameByCallId = new Map<string, string>()
    for (const event of input.events) {
      if (event.type !== 'tool/call' && event.type !== 'tool/result') continue
      if (event.type === 'tool/call') {
        nameByCallId.set(String(event.data.callId), event.data.name)
        continue
      }
      // A tool/result's message source is always tool-typed; the call that
      // named the tool is paired through its callId.
      const name = nameByCallId.get(String(event.data.message.source.callId)) ?? 'tool'
      const basis = basisFromMeta(event.data.meta)
      if (basis !== undefined) {
        const abs = workspacePath(input.cwd, basis.path)
        if (abs === null) {
          report.conflicts.push(basis.path)
          continue
        }
        plans.push({ basis, abs })
        continue
      }
      if (SHELL_TOOL_NAMES.has(name)) {
        shellNames.add(name)
        report.shell.count += 1
        continue
      }
      if (BASISLESS_FS_TOOLS.has(name)) {
        toolNames.add(name)
        report.notRestorable.count += 1
        continue
      }
    }
    for (const plan of plans.reverse()) await this.apply(plan, report)
    report.notRestorable.toolNames = [...toolNames]
    report.shell.names = [...shellNames]
    return report
  }

  private async apply(plan: RestorePlan, report: TurnRestoreReport): Promise<void> {
    const { basis, abs } = plan
    let current: string | null
    try {
      current = await readFile(abs, 'utf8')
    } catch {
      current = null
    }
    const afters = basis.after === null ? null : normalizeLineEndings(basis.after)
    if (basis.op === 'create') {
      // A missing file already matches the create's pre-state; an existing file
      // only falls when its content still equals what the turn wrote.
      if (current === null) {
        report.restored += 1
        return
      }
      if (afters === null) {
        report.notRestorable.count += 1
        return
      }
      if (normalizeLineEndings(current) !== afters) {
        report.conflicts.push(basis.path)
        return
      }
      try {
        await rm(abs)
      } catch {
        report.conflicts.push(basis.path)
      }
      report.restored += 1
      return
    }
    if (basis.before === null || afters === null) {
      // Size-capped write: the before-text was refused at capture time.
      report.notRestorable.count += 1
      return
    }
    if (current === null) {
      report.conflicts.push(basis.path)
      return
    }
    if (normalizeLineEndings(current) !== afters) {
      report.conflicts.push(basis.path)
      return
    }
    await writeFile(abs, basis.before, 'utf8')
    report.restored += 1
  }
}

/** The empty report used when a restore could not run at all (busy source, missing cwd). */
export const EMPTY_TURN_RESTORE_REPORT: TurnRestoreReport = {
  restored: 0,
  conflicts: [],
  notRestorable: { count: 0, toolNames: [] },
  shell: { count: 0, names: [] },
}

/**
 * Resolve a display path against the workspace root and confirm it stays
 * inside it. Absolute paths are accepted only when they already live under the
 * root; `.` is the root itself.
 * @param cwd - workspace root.
 * @param displayPath - the tool's display path.
 * @returns the absolute path, or null when it escapes the workspace.
 */
export function workspacePath(cwd: string, displayPath: string): string | null {
  const resolved = path.resolve(cwd, displayPath)
  const relative = path.relative(cwd, resolved)
  if (relative === '') return resolved
  if (relative.startsWith('..') || path.isAbsolute(relative)) return null
  return resolved
}

function normalizeLineEndings(text: string): string {
  return text.replace(/\r\n/gu, '\n')
}

/** Cordis plugin name used by Loader diagnostics. */
export const name = 'turn-restore'

/**
 * Install the turn-restore service. The host's `session.fork` consumes it
 * through `ctx.get('turnRestore')` (optional integration: without this plugin
 * composed, a rewind rewinds the conversation only).
 * @param ctx - plugin context.
 */
export function apply(ctx: Context): void {
  ctx.provide('turnRestore', new TurnRestore())
}

declare module '@deepseek-ai/cordis' {
  interface Context {
    /** Rewind-time workspace restore service (dsh-turn-restore); absent when the plugin is not composed. */
    turnRestore: TurnRestore
  }
}
