/** Durable change summaries: one document per Session, so a Host that did not record a turn can still serve its file list. */
import { z } from 'zod'
import type { SessionId } from '@deepseek-ai/dsh-session'
import type { Domain, DomainFacility } from '@deepseek-ai/dsh-storage-domain'
import { defineDomain, domainTable } from '@deepseek-ai/dsh-storage-domain'
import type { WorkspaceChangedFile, WorkspaceChangesSummary } from './types.ts'

/** One changed file as stored: its path, its label, and the turn's line counts. */
const changedFileSchema = z.object({
  path: z.string().min(1),
  display: z.string().min(1),
  added: z.number().int().nonnegative(),
  deleted: z.number().int().nonnegative(),
  binary: z.literal(true).optional(),
  oversized: z.literal(true).optional(),
}).strict()

/** One turn's stored summary; the git snapshot ids are absent because their objects die with the recorder. */
const summarySchema = z.object({
  turn: z.number().int().positive(),
  cwd: z.string().min(1),
  files: z.array(changedFileSchema),
  total: z.number().int().nonnegative(),
  added: z.number().int().nonnegative(),
  deleted: z.number().int().nonnegative(),
}).strict()

/** One Session's stored summaries, keyed by the sequence of the event that announced them. */
const sessionRecordSchema = z.object({
  summaries: z.record(z.string(), summarySchema),
}).strict()

/** The stored record of one Session's change summaries. */
export type WorkspaceChangesSessionRecord = z.infer<typeof sessionRecordSchema>

/**
 * The workspace-changes domain: one document per Session, each holding that
 * Session's recorded summaries. The Session log's `workspace/changes` event
 * stays the announcement; these documents hold the file list the event names.
 * They are derived data, so a record that fails its schema is moved aside
 * instead of failing the boot.
 */
export const workspaceChangesDomain = defineDomain({
  name: 'workspace_changes',
  version: 1,
  invalidRecords: 'backup-and-skip',
  layout: 'per-record',
  tables: { sessions: domainTable<SessionId, WorkspaceChangesSessionRecord>(sessionRecordSchema) },
})

/** The stored form of one changed file. */
function storedFile(file: WorkspaceChangedFile): z.infer<typeof changedFileSchema> {
  return {
    path: file.path,
    display: file.display,
    added: file.added,
    deleted: file.deleted,
    ...file.binary === true ? { binary: true } : {},
    ...file.oversized === true ? { oversized: true } : {},
  }
}

/** The stored form of one summary: the file list and the counts, without the git snapshot ids. */
function storedSummary(summary: WorkspaceChangesSummary): z.infer<typeof summarySchema> {
  return {
    turn: summary.turn,
    cwd: summary.cwd,
    files: summary.files.map(storedFile),
    total: summary.total,
    added: summary.added,
    deleted: summary.deleted,
  }
}

/** The served form of one stored summary. */
function servedSummary(summary: z.infer<typeof summarySchema>): WorkspaceChangesSummary {
  return {
    turn: summary.turn,
    cwd: summary.cwd,
    files: summary.files.map(file => ({
      path: file.path,
      display: file.display,
      added: file.added,
      deleted: file.deleted,
      ...file.binary === true ? { binary: true as const } : {},
      ...file.oversized === true ? { oversized: true as const } : {},
    })),
    total: summary.total,
    added: summary.added,
    deleted: summary.deleted,
  }
}

/**
 * The durable store of recorded change summaries. One document holds one
 * Session's summaries; a write replaces that document.
 */
export class WorkspaceChangesStore {
  private constructor(private readonly domain: Domain<typeof workspaceChangesDomain> | undefined) {}

  /**
   * Open the store, or report that this Host cannot use it.
   * @param facility - the storage facility that opens the domain.
   * @param warn - reports a store this Host cannot use.
   * @returns the store; its summaries stay absent when the domain did not open.
   */
  static async open(facility: DomainFacility, warn: (message: string) => void): Promise<WorkspaceChangesStore> {
    try {
      return new WorkspaceChangesStore(await facility.open(workspaceChangesDomain))
    } catch (error: unknown) {
      warn(`workspace-changes: the durable change-summary store is unavailable; summaries stay in this process only (${String(error)})`)
      return new WorkspaceChangesStore(undefined)
    }
  }

  /**
   * The stored summary of one announced turn.
   * @param sessionId - the Session that appended the event.
   * @param seq - the event's sequence number.
   * @returns the summary, or undefined when this store holds none.
   */
  summary(sessionId: SessionId, seq: number): WorkspaceChangesSummary | undefined {
    const stored = this.domain?.table('sessions').get(sessionId)?.summaries[String(seq)]
    return stored === undefined ? undefined : servedSummary(stored)
  }

  /**
   * Store one announced turn's summary, so a later Host can serve its file list.
   * @param sessionId - the Session that appended the event.
   * @param seq - the event's sequence number.
   * @param summary - the summary to store.
   */
  async persist(sessionId: SessionId, seq: number, summary: WorkspaceChangesSummary): Promise<void> {
    const domain = this.domain
    if (domain === undefined) return
    const table = domain.table('sessions')
    const record = table.get(sessionId) ?? { summaries: {} }
    await table.put(sessionId, { summaries: { ...record.summaries, [String(seq)]: storedSummary(summary) } })
  }

  /** Close the domain and release its medium. */
  async close(): Promise<void> {
    await this.domain?.close()
  }
}
