/**
 * Hash-chained audit log. Every audited action appends one record whose hash
 * covers its content and the previous record's hash, so a modified or deleted
 * record breaks verification of every later record.
 * @module @deepseek-ai/dsh-ncag-store/audit
 */

import { createHash } from 'node:crypto'
import type { DatabaseSync } from 'node:sqlite'
import { asRows, decodeJson, encodeJson } from './columns.ts'

/** Audit categories recorded for externally visible actions. */
export type StorageAuditCategory =
  | 'request'
  | 'planning'
  | 'agent'
  | 'task'
  | 'message'
  | 'resource'
  | 'tool'
  | 'code'
  | 'review'
  | 'decision'
  | 'recovery'
  | 'cost'
  | 'delivery'
  | 'security'

/** One stored audit record. */
export interface AuditEntry {
  /** Record identifier. */
  readonly id: string
  /** Monotonic sequence number; a gap between records indicates loss. */
  readonly sequence: number
  /** UTC timestamp. */
  readonly at: string
  /** Audit category. */
  readonly category: StorageAuditCategory
  /** Action name inside the category. */
  readonly action: string
  /** Actor identity: an agent id, the orchestrator, or a user handle. */
  readonly actor: string
  /** Related project, when the action belongs to one. */
  readonly projectId: string | null
  /** Related task, when the action belongs to one. */
  readonly taskId: string | null
  /** Related agent, when the action belongs to one. */
  readonly agentId: string | null
  /** Correlation id tying this action to one logical operation. */
  readonly correlationId: string
  /** Structured detail; never contains credentials. */
  readonly detail: Record<string, unknown>
  /** Hash of the previous record. */
  readonly previousHash: string
  /** Hash over this record. */
  readonly hash: string
}

/** The genesis hash preceding the first record. */
export const AUDIT_GENESIS_HASH = createHash('sha256').update('ncag-audit-genesis').digest('hex')

/** Fields an audited action supplies; the store owns id, sequence and hashes. */
export interface AuditInput {
  readonly category: StorageAuditCategory
  readonly action: string
  readonly actor: string
  readonly projectId?: string | null
  readonly taskId?: string | null
  readonly agentId?: string | null
  readonly correlationId: string
  readonly detail?: Record<string, unknown>
}

/**
 * Compute one record's hash from its canonical content and the previous hash.
 * @param body - the record fields in a fixed order.
 * @param previousHash - hash of the preceding record.
 * @returns the SHA-256 hex digest.
 */
function hashRecord(body: readonly unknown[], previousHash: string): string {
  return createHash('sha256').update(JSON.stringify([previousHash, ...body])).digest('hex')
}

/**
 * Append-only audit log over the platform database. The store owns one
 * instance; every other module records actions through it.
 */
export class AuditLog {
  private lastHash: string | undefined
  private lastSequence: number = 0

  /**
   * @param db - open platform database.
   * @param onRecord - notification invoked after a record commits.
   */
  private readonly db: DatabaseSync
  private readonly onRecord: (entry: AuditEntry) => void

  /**
   * @param db - open platform database.
   * @param onRecord - notification invoked after a record commits.
   */
  constructor(db: DatabaseSync, onRecord: (entry: AuditEntry) => void) {
    this.db = db
    this.onRecord = onRecord
  }

  /**
   * Append one audit record and return it.
   * @param input - the audited action.
   * @returns the stored record, including its sequence and hashes.
   */
  append(input: AuditInput): AuditEntry {
    const state = this.state()
    const sequence = state.sequence + 1
    const at = new Date().toISOString()
    const id = `aud_${String(sequence).padStart(8, '0')}_${at}`
    const detail = input.detail ?? {}
    const previousHash = state.hash
    const hash = hashRecord([
      sequence, id, at, input.category, input.action, input.actor,
      input.projectId ?? null, input.taskId ?? null, input.agentId ?? null,
      input.correlationId, detail,
    ], previousHash)
    const entry: AuditEntry = {
      id, sequence, at, category: input.category, action: input.action, actor: input.actor,
      projectId: input.projectId ?? null, taskId: input.taskId ?? null, agentId: input.agentId ?? null,
      correlationId: input.correlationId, detail, previousHash, hash,
    }
    this.db.prepare(`INSERT INTO audit (id, at, category, action, actor, project_id, task_id, agent_id, correlation_id, detail, previous_hash, hash)
      VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`).run(
      entry.id, entry.at, entry.category, entry.action, entry.actor,
      entry.projectId, entry.taskId, entry.agentId, entry.correlationId,
      encodeJson(detail), entry.previousHash, entry.hash,
    )
    this.lastHash = hash
    this.lastSequence = sequence
    this.onRecord(entry)
    return entry
  }

  /** Current chain tip, read from the database on first use. */
  private state(): { sequence: number; hash: string } {
    if (this.lastHash === undefined) {
      const row = this.db.prepare('SELECT sequence, hash FROM audit ORDER BY sequence DESC LIMIT 1').get() as
        | { sequence: number; hash: string }
        | undefined
      this.lastHash = row?.hash ?? AUDIT_GENESIS_HASH
      this.lastSequence = row?.sequence ?? 0
    }
    return { sequence: this.lastSequence, hash: this.lastHash }
  }

  /**
   * Read records in sequence order.
   * @param limit - maximum records to return, newest first before ordering.
   * @returns the requested records, oldest first.
   */
  tail(limit: number): AuditEntry[] {
    const rows = asRows<AuditRow>(this.db.prepare('SELECT * FROM audit ORDER BY sequence DESC LIMIT ?').all(limit))
    return rows.reverse().map(rowToEntry)
  }

  /**
   * Read the records recorded for one correlation identifier.
   * @param projectId - project scoping the trace.
   * @param correlationId - correlation identifier shared by the recorded actions.
   * @returns matching records in sequence order.
   */
  forCorrelation(projectId: string, correlationId: string): AuditEntry[] {
    const rows = asRows<AuditRow>(this.db.prepare(
      'SELECT * FROM audit WHERE project_id = ? AND correlation_id = ? ORDER BY sequence ASC',
    ).all(projectId, correlationId))
    return rows.map(rowToEntry)
  }

  /**
   * Verify the whole chain and report the first inconsistency.
   * @returns the record count and the sequence of the first broken record, or null when intact.
   */
  verify(): { records: number; brokenAt: number | null } {
    const rows = asRows<AuditRow>(this.db.prepare('SELECT * FROM audit ORDER BY sequence ASC').all())
    let expectedPrevious = AUDIT_GENESIS_HASH
    let expectedSequence = 1
    for (const row of rows) {
      const entry = rowToEntry(row)
      const recomputed = hashRecord([
        entry.sequence, entry.id, entry.at, entry.category, entry.action, entry.actor,
        entry.projectId, entry.taskId, entry.agentId, entry.correlationId, entry.detail,
      ], expectedPrevious)
      if (row.sequence !== expectedSequence || entry.previousHash !== expectedPrevious || recomputed !== entry.hash) {
        return { records: rows.length, brokenAt: row.sequence }
      }
      expectedPrevious = entry.hash
      expectedSequence += 1
    }
    return { records: rows.length, brokenAt: null }
  }
}

/** Physical row of the audit table. */
interface AuditRow {
  readonly sequence: number
  readonly id: string
  readonly at: string
  readonly category: string
  readonly action: string
  readonly actor: string
  readonly project_id: string | null
  readonly task_id: string | null
  readonly agent_id: string | null
  readonly correlation_id: string
  readonly detail: string
  readonly previous_hash: string
  readonly hash: string
}

/** Map one physical row to an {@link AuditEntry}. */
function rowToEntry(row: AuditRow): AuditEntry {
  return {
    id: row.id, sequence: row.sequence, at: row.at,
    category: row.category as StorageAuditCategory, action: row.action, actor: row.actor,
    projectId: row.project_id, taskId: row.task_id, agentId: row.agent_id,
    correlationId: row.correlation_id,
    detail: decodeJson<Record<string, unknown>>(row.detail, {}),
    previousHash: row.previous_hash, hash: row.hash,
  }
}
