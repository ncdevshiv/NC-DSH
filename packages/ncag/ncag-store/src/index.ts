/**
 * Platform store: one SQLite database holding every durable NC-AG record, the
 * repositories that read and write them, and the hash-chained audit log.
 *
 * The store is a plain class, not a Cordis service: the orchestrator owns one
 * instance and passes it to the modules it drives, so no module reaches for
 * process-global state.
 * @module @deepseek-ai/dsh-ncag-store
 */

import type { DatabaseSync } from 'node:sqlite'
import { AuditLog } from './audit.ts'
import type { AuditEntry, AuditInput } from './audit.ts'
import { asRows } from './columns.ts'
import { openPlatformDatabase, NCAG_SCHEMA_VERSION } from './schema.ts'

export { AuditLog, AUDIT_GENESIS_HASH } from './audit.ts'
export type { AuditEntry, AuditInput } from './audit.ts'
export { NCAG_SCHEMA_VERSION, MIGRATIONS, openPlatformDatabase } from './schema.ts'
export type { Migration } from './schema.ts'
export * from './columns.ts'
export * from './repositories.ts'

/** One platform event appended to the ordered event stream. */
export interface EventRecord {
  /** Monotonic sequence in the event stream. */
  readonly sequence: number
  /** UTC timestamp. */
  readonly at: string
  /** Event kind, for example `task/transition`. */
  readonly kind: string
  readonly projectId: string | null
  readonly taskId: string | null
  readonly agentId: string | null
  /** One-line human summary shown in the timeline. */
  readonly summary: string
  /** Structured payload of the event. */
  readonly data: Record<string, unknown>
}

/** Fields an emitter supplies; sequence and timestamp are store-owned. */
export interface EventInput {
  readonly kind: string
  readonly projectId?: string | null
  readonly taskId?: string | null
  readonly agentId?: string | null
  readonly summary: string
  readonly data?: Record<string, unknown>
}

/**
 * The platform store. One instance owns one database connection and one audit
 * chain; repositories are thin record mappers over the same connection.
 */
export class NcagStore {
  /** The open database handle, for repositories that need a statement. */
  readonly db: DatabaseSync
  /** Hash-chained audit log. */
  readonly audit: AuditLog
  /** Physical schema version of the open database. */
  readonly schemaVersion: number = NCAG_SCHEMA_VERSION
  private readonly listeners = new Set<(event: EventRecord) => void>()
  private closed: boolean = false

  /**
   * @param db - open platform database.
   */
  constructor(db: DatabaseSync) {
    this.db = db
    this.audit = new AuditLog(db, (entry) => {
      this.notifyAudit(entry)
    })
  }

  /**
   * Open a store at one path.
   * @param path - database file path or `:memory:`.
   * @returns the opened store.
   */
  static open(path: string): NcagStore {
    return new NcagStore(openPlatformDatabase(path))
  }

  /**
   * Append one platform event and notify subscribers.
   * @param input - the event to record.
   * @returns the stored event with its sequence and timestamp.
   */
  emitEvent(input: EventInput): EventRecord {
    const at = new Date().toISOString()
    const data = input.data ?? {}
    const record: EventRecord = {
      sequence: this.insertEvent(at, input.kind, input.projectId ?? null, input.taskId ?? null, input.agentId ?? null, input.summary, data),
      at, kind: input.kind,
      projectId: input.projectId ?? null, taskId: input.taskId ?? null, agentId: input.agentId ?? null,
      summary: input.summary, data,
    }
    for (const listener of this.listeners) listener(record)
    return record
  }

  /**
   * Subscribe to platform events.
   * @param listener - invoked for every event after it commits.
   * @returns the disposer that unsubscribes.
   */
  subscribe(listener: (event: EventRecord) => void): () => void {
    this.listeners.add(listener)
    return () => { this.listeners.delete(listener) }
  }

  /**
   * Read the most recent events, oldest first.
   * @param limit - maximum number of events.
   * @param projectId - optional project filter.
   * @returns the requested events.
   */
  recentEvents(limit: number, projectId?: string): EventRecord[] {
    const rows = projectId === undefined
      ? this.db.prepare('SELECT * FROM platform_events ORDER BY sequence DESC LIMIT ?').all(limit)
      : this.db.prepare('SELECT * FROM platform_events WHERE project_id = ? ORDER BY sequence DESC LIMIT ?').all(projectId, limit)
    return asRows<EventRow>(rows).reverse().map(rowToEvent)
  }

  /**
   * Read the events recorded for one correlation identifier.
   * @param projectId - project scoping the trace.
   * @param correlationId - correlation identifier carried in the event data.
   * @returns matching events in sequence order.
   */
  eventsForCorrelation(projectId: string, correlationId: string): EventRecord[] {
    const rows = asRows<EventRow>(this.db.prepare(
      "SELECT * FROM platform_events WHERE project_id = ? AND json_extract(data, '$.correlationId') = ? ORDER BY sequence ASC",
    ).all(projectId, correlationId))
    return rows.map(rowToEvent)
  }

  /**
   * Record one audited action.
   * @param input - the action to audit.
   * @returns the stored audit entry.
   */
  recordAudit(input: AuditInput): AuditEntry {
    return this.audit.append(input)
  }

  /**
   * Verify the audit chain.
   * @returns the record count and the first broken sequence, or null when intact.
   */
  verifyAudit(): { records: number; brokenAt: number | null } {
    return this.audit.verify()
  }

  /**
   * Close the database. Further calls on this store reject.
   */
  close(): void {
    if (this.closed) return
    this.closed = true
    this.listeners.clear()
    this.db.close()
  }

  /** Forward one audit record to the event timeline and its subscribers. */
  private notifyAudit(entry: AuditEntry): void {
    const sequence = this.insertEvent(
      entry.at,
      `audit/${entry.category}`,
      entry.projectId,
      entry.taskId,
      entry.agentId,
      `${entry.category}: ${entry.action}`,
      { action: entry.action, actor: entry.actor, correlationId: entry.correlationId },
    )
    const record: EventRecord = {
      sequence,
      at: entry.at,
      kind: `audit/${entry.category}`,
      projectId: entry.projectId,
      taskId: entry.taskId,
      agentId: entry.agentId,
      summary: `${entry.category}: ${entry.action}`,
      data: { action: entry.action, actor: entry.actor, correlationId: entry.correlationId },
    }
    for (const listener of this.listeners) listener(record)
  }

  /** Append one row to the platform event table and return its sequence. */
  private insertEvent(
    at: string,
    kind: string,
    projectId: string | null,
    taskId: string | null,
    agentId: string | null,
    summary: string,
    data: Record<string, unknown>,
  ): number {
    this.db.prepare(`INSERT INTO platform_events (at, kind, project_id, task_id, agent_id, summary, data)
      VALUES (?, ?, ?, ?, ?, ?, ?)`).run(at, kind, projectId, taskId, agentId, summary, JSON.stringify(data))
    const row = this.db.prepare('SELECT last_insert_rowid() AS seq').get() as { seq: number }
    return row.seq
  }
}

/** Physical row of the platform event table. */
interface EventRow {
  readonly sequence: number
  readonly at: string
  readonly kind: string
  readonly project_id: string | null
  readonly task_id: string | null
  readonly agent_id: string | null
  readonly summary: string
  readonly data: string
}

/** Map one physical row to an {@link EventRecord}. */
function rowToEvent(row: EventRow): EventRecord {
  return {
    sequence: row.sequence, at: row.at, kind: row.kind,
    projectId: row.project_id, taskId: row.task_id, agentId: row.agent_id,
    summary: row.summary, data: JSON.parse(row.data) as Record<string, unknown>,
  }
}
