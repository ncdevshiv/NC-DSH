/**
 * Physical schema for the NC-AG platform database: DDL, the schema version,
 * forward migrations, and the open sequence every store instance shares.
 *
 * A fresh database is created at {@link NCAG_SCHEMA_VERSION}. An existing
 * database at an older version is migrated forward through the ordered
 * {@link MIGRATIONS} list; a database at a newer version is rejected instead
 * of being silently downgraded.
 * @module @deepseek-ai/dsh-ncag-store/schema
 */

import { DatabaseSync } from 'node:sqlite'
import { mkdirSync } from 'node:fs'
import { dirname, resolve } from 'node:path'

/** Current physical schema version stored in PRAGMA user_version. */
export const NCAG_SCHEMA_VERSION = 8

/** One ordered forward migration. */
export interface Migration {
  /** Target version this migration produces. */
  readonly version: number
  /** Short description recorded in the schema history. */
  readonly description: string
  /** Apply the migration to an open database. */
  readonly apply: (db: DatabaseSync) => void
}

/** Tables the platform owns, in creation order. */
const CORE_DDL: readonly string[] = [
  `CREATE TABLE IF NOT EXISTS ncag_meta (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL
) STRICT`,
  `CREATE TABLE IF NOT EXISTS schema_history (
  version     INTEGER PRIMARY KEY,
  description TEXT NOT NULL,
  applied_at  TEXT NOT NULL
) STRICT`,
  `CREATE TABLE IF NOT EXISTS projects (
  id              TEXT PRIMARY KEY,
  name            TEXT NOT NULL,
  description     TEXT NOT NULL,
  repository_path TEXT NOT NULL,
  base_branch     TEXT NOT NULL,
  status          TEXT NOT NULL,
  instructions    TEXT NOT NULL,
  autonomy        TEXT NOT NULL,
  context         TEXT NOT NULL,
  checks          TEXT NOT NULL,
  usage           TEXT NOT NULL,
  created_at      TEXT NOT NULL,
  updated_at      TEXT NOT NULL
) STRICT`,
  `CREATE TABLE IF NOT EXISTS requests (
  id          TEXT PRIMARY KEY,
  project_id  TEXT NOT NULL REFERENCES projects(id),
  raw_text    TEXT NOT NULL,
  created_at  TEXT NOT NULL,
  intent      TEXT NOT NULL,
  ambiguities TEXT NOT NULL,
  status      TEXT NOT NULL
) STRICT`,
  `CREATE TABLE IF NOT EXISTS requirements (
  id           TEXT PRIMARY KEY,
  request_id   TEXT NOT NULL,
  project_id   TEXT NOT NULL REFERENCES projects(id),
  description  TEXT NOT NULL,
  category     TEXT NOT NULL,
  priority     TEXT NOT NULL,
  source       TEXT NOT NULL,
  dependencies TEXT NOT NULL,
  acceptance   TEXT NOT NULL,
  status       TEXT NOT NULL
) STRICT`,
  `CREATE TABLE IF NOT EXISTS profiles (
  id       TEXT PRIMARY KEY,
  key      TEXT NOT NULL UNIQUE,
  payload  TEXT NOT NULL,
  built_in INTEGER NOT NULL
) STRICT`,
  `CREATE TABLE IF NOT EXISTS agents (
  id                 TEXT PRIMARY KEY,
  profile_id         TEXT NOT NULL,
  profile_key        TEXT NOT NULL,
  project_id         TEXT NOT NULL REFERENCES projects(id),
  task_id            TEXT,
  status             TEXT NOT NULL,
  status_reason      TEXT,
  workspace_path     TEXT,
  branch             TEXT,
  model              TEXT NOT NULL,
  capabilities       TEXT NOT NULL,
  usage              TEXT NOT NULL,
  failure_count      INTEGER NOT NULL,
  steps              INTEGER NOT NULL,
  active_correlation TEXT,
  created_at         TEXT NOT NULL,
  updated_at         TEXT NOT NULL,
  last_heartbeat_at  TEXT NOT NULL,
  last_activity_at   TEXT NOT NULL,
  stopped_at         TEXT
) STRICT`,
  `CREATE TABLE IF NOT EXISTS tasks (
  id                    TEXT PRIMARY KEY,
  project_id            TEXT NOT NULL REFERENCES projects(id),
  request_id            TEXT,
  requirement_ids       TEXT NOT NULL,
  title                 TEXT NOT NULL,
  description           TEXT NOT NULL,
  required_capability   TEXT NOT NULL,
  assigned_agent_id     TEXT,
  dependency_ids        TEXT NOT NULL,
  required_inputs       TEXT NOT NULL,
  expected_outputs      TEXT NOT NULL,
  acceptance_criteria   TEXT NOT NULL,
  required_permissions  TEXT NOT NULL,
  context_refs          TEXT NOT NULL,
  priority              TEXT NOT NULL,
  deadline_ms           INTEGER,
  required_reviews      TEXT NOT NULL,
  required_verifications TEXT NOT NULL,
  status                TEXT NOT NULL,
  status_reason         TEXT,
  depth                 INTEGER NOT NULL,
  attempts              INTEGER NOT NULL,
  review_cycles         INTEGER NOT NULL,
  consultation          INTEGER NOT NULL DEFAULT 0,
  created_at            TEXT NOT NULL,
  updated_at            TEXT NOT NULL,
  started_at            TEXT,
  completed_at          TEXT
) STRICT`,
  `CREATE TABLE IF NOT EXISTS messages (
  id                 TEXT PRIMARY KEY,
  at                 TEXT NOT NULL,
  sender             TEXT,
  target             TEXT NOT NULL,
  project_id         TEXT NOT NULL REFERENCES projects(id),
  task_id            TEXT,
  type               TEXT NOT NULL,
  subject            TEXT NOT NULL,
  content            TEXT NOT NULL,
  context_refs       TEXT NOT NULL,
  correlation_id     TEXT NOT NULL,
  priority           TEXT NOT NULL,
  requires_ack       INTEGER NOT NULL,
  delivery_status    TEXT NOT NULL,
  delivered_at       TEXT,
  acknowledged_at    TEXT,
  attempts           INTEGER NOT NULL,
  expires_at_ms      INTEGER,
  dead_letter_reason TEXT
) STRICT`,
  `CREATE TABLE IF NOT EXISTS decisions (
  id                      TEXT PRIMARY KEY,
  project_id              TEXT NOT NULL REFERENCES projects(id),
  task_id                 TEXT,
  category                TEXT NOT NULL,
  author                  TEXT NOT NULL,
  problem                 TEXT NOT NULL,
  debate                  TEXT,
  recommendation          TEXT NOT NULL,
  outcome                 TEXT NOT NULL,
  rationale               TEXT,
  rejected_recommendation TEXT,
  consequences            TEXT NOT NULL,
  affected_modules        TEXT NOT NULL,
  affected_task_ids       TEXT NOT NULL,
  status                  TEXT NOT NULL,
  review_history          TEXT NOT NULL,
  created_at              TEXT NOT NULL,
  decided_at              TEXT
) STRICT`,
  `CREATE TABLE IF NOT EXISTS findings (
  id                TEXT PRIMARY KEY,
  project_id        TEXT NOT NULL REFERENCES projects(id),
  task_id           TEXT NOT NULL,
  reviewer_agent_id TEXT NOT NULL,
  review_kind       TEXT NOT NULL DEFAULT 'code-review',
  severity          TEXT NOT NULL,
  description       TEXT NOT NULL,
  evidence          TEXT NOT NULL,
  artifact          TEXT NOT NULL,
  impact            TEXT NOT NULL,
  recommendation    TEXT NOT NULL,
  verification      TEXT NOT NULL,
  status            TEXT NOT NULL,
  created_at        TEXT NOT NULL,
  resolved_at       TEXT
) STRICT`,
  `CREATE TABLE IF NOT EXISTS review_runs (
  id                TEXT PRIMARY KEY,
  project_id        TEXT NOT NULL REFERENCES projects(id),
  task_id           TEXT NOT NULL,
  review_kind       TEXT NOT NULL,
  reviewer_agent_id TEXT NOT NULL,
  summary           TEXT NOT NULL,
  findings          INTEGER NOT NULL,
  rejected          INTEGER NOT NULL,
  created_at        TEXT NOT NULL
) STRICT`,
  `CREATE TABLE IF NOT EXISTS evidence (
  sequence     INTEGER PRIMARY KEY AUTOINCREMENT,
  task_id      TEXT NOT NULL,
  project_id   TEXT NOT NULL REFERENCES projects(id),
  criterion_id TEXT NOT NULL,
  method       TEXT NOT NULL,
  status       TEXT NOT NULL,
  command      TEXT,
  exit_code    INTEGER,
  output       TEXT NOT NULL,
  recorded_at  TEXT NOT NULL,
  produced_by  TEXT
) STRICT`,
  `CREATE TABLE IF NOT EXISTS leases (
  id             TEXT PRIMARY KEY,
  project_id     TEXT NOT NULL REFERENCES projects(id),
  resource_path  TEXT NOT NULL,
  mode           TEXT NOT NULL,
  agent_id       TEXT NOT NULL,
  task_id        TEXT,
  fencing_token  INTEGER NOT NULL,
  acquired_at_ms INTEGER NOT NULL,
  renewed_at_ms  INTEGER NOT NULL,
  expires_at_ms  INTEGER NOT NULL,
  status         TEXT NOT NULL,
  reason         TEXT NOT NULL
) STRICT`,
  `CREATE TABLE IF NOT EXISTS lease_requests (
  project_id    TEXT NOT NULL REFERENCES projects(id),
  resource_path TEXT NOT NULL,
  agent_id      TEXT NOT NULL,
  task_id       TEXT,
  mode          TEXT NOT NULL,
  reason        TEXT NOT NULL,
  requested_at  TEXT NOT NULL,
  PRIMARY KEY (project_id, resource_path, agent_id, mode)
) STRICT`,
  `CREATE TABLE IF NOT EXISTS changesets (
  id               TEXT PRIMARY KEY,
  project_id       TEXT NOT NULL REFERENCES projects(id),
  task_id          TEXT NOT NULL,
  agent_id         TEXT NOT NULL,
  workspace_path   TEXT NOT NULL,
  branch           TEXT NOT NULL,
  base_commit      TEXT NOT NULL,
  head_commit      TEXT,
  changed_files    TEXT NOT NULL,
  message          TEXT NOT NULL,
  status           TEXT NOT NULL,
  submitted_at     TEXT NOT NULL,
  report           TEXT NOT NULL,
  integrated_commit TEXT
) STRICT`,
  `CREATE TABLE IF NOT EXISTS workspaces (
  agent_id    TEXT PRIMARY KEY,
  project_id  TEXT NOT NULL REFERENCES projects(id),
  task_id     TEXT,
  path        TEXT NOT NULL,
  branch      TEXT NOT NULL,
  base_commit TEXT NOT NULL,
  created_at  TEXT NOT NULL,
  released_at TEXT
) STRICT`,
  `CREATE TABLE IF NOT EXISTS audit (
  sequence       INTEGER PRIMARY KEY AUTOINCREMENT,
  id             TEXT NOT NULL UNIQUE,
  at             TEXT NOT NULL,
  category       TEXT NOT NULL,
  action         TEXT NOT NULL,
  actor          TEXT NOT NULL,
  project_id     TEXT,
  task_id        TEXT,
  agent_id       TEXT,
  correlation_id TEXT NOT NULL,
  detail         TEXT NOT NULL,
  previous_hash  TEXT NOT NULL,
  hash           TEXT NOT NULL
) STRICT`,
  `CREATE TABLE IF NOT EXISTS platform_events (
  sequence   INTEGER PRIMARY KEY AUTOINCREMENT,
  at         TEXT NOT NULL,
  kind       TEXT NOT NULL,
  project_id TEXT,
  task_id    TEXT,
  agent_id   TEXT,
  summary    TEXT NOT NULL,
  data       TEXT NOT NULL
) STRICT`,
  `CREATE TABLE IF NOT EXISTS tool_calls (
  sequence       INTEGER PRIMARY KEY AUTOINCREMENT,
  at             TEXT NOT NULL,
  agent_id       TEXT NOT NULL,
  project_id     TEXT NOT NULL,
  task_id        TEXT,
  tool           TEXT NOT NULL,
  args           TEXT NOT NULL,
  ok             INTEGER NOT NULL,
  error          TEXT,
  output         TEXT NOT NULL,
  duration_ms    INTEGER NOT NULL,
  correlation_id TEXT NOT NULL
) STRICT`,
  `CREATE TABLE IF NOT EXISTS usage_ledger (
  sequence       INTEGER PRIMARY KEY AUTOINCREMENT,
  at             TEXT NOT NULL,
  project_id     TEXT NOT NULL REFERENCES projects(id),
  task_id        TEXT,
  agent_id       TEXT,
  provider       TEXT NOT NULL,
  model          TEXT NOT NULL,
  input_tokens   INTEGER NOT NULL,
  output_tokens  INTEGER NOT NULL,
  cached_tokens  INTEGER NOT NULL,
  cost_usd       REAL NOT NULL,
  purpose        TEXT NOT NULL
) STRICT`,
  `CREATE TABLE IF NOT EXISTS checkpoints (
  id           TEXT PRIMARY KEY,
  project_id   TEXT NOT NULL REFERENCES projects(id),
  at           TEXT NOT NULL,
  reason       TEXT NOT NULL,
  state        TEXT NOT NULL
) STRICT`,
  `CREATE TABLE IF NOT EXISTS tool_invocations (
  agent_id    TEXT NOT NULL,
  fingerprint TEXT NOT NULL,
  count       INTEGER NOT NULL,
  first_at    TEXT NOT NULL,
  last_at     TEXT NOT NULL,
  PRIMARY KEY (agent_id, fingerprint)
) STRICT`,
  `CREATE TABLE IF NOT EXISTS messages_acks (
  message_id TEXT NOT NULL,
  agent_id   TEXT NOT NULL,
  at         TEXT NOT NULL,
  PRIMARY KEY (message_id, agent_id)
) STRICT`,
  `CREATE TABLE IF NOT EXISTS message_cursors (
  agent_id   TEXT PRIMARY KEY,
  last_seen  INTEGER NOT NULL,
  updated_at TEXT NOT NULL
) STRICT`,
]

/**
 * Ordered forward migrations. Version 2 introduced per-agent message
 * acknowledgement cursors; version 3 added the tool-invocation ledger used by
 * stuck-agent detection; version 4 added the project verification command
 * table; version 5 added the consultation task flag; version 6 records the
 * review kind that produced a finding; version 8 adds the wait queue for
 * leased resources. A database created at the
 * current version already carries every table, so the list only runs on older
 * media.
 */
export const MIGRATIONS: readonly Migration[] = [
  {
    version: 2,
    description: 'add message acknowledgement cursors',
    apply: (db) => {
      db.exec(`CREATE TABLE IF NOT EXISTS message_cursors (
  agent_id   TEXT PRIMARY KEY,
  last_seen  INTEGER NOT NULL,
  updated_at TEXT NOT NULL
) STRICT`)
    },
  },
  {
    version: 3,
    description: 'add tool invocation fingerprints for loop detection',
    apply: (db) => {
      db.exec(`CREATE TABLE IF NOT EXISTS tool_invocations (
  agent_id    TEXT NOT NULL,
  fingerprint TEXT NOT NULL,
  count       INTEGER NOT NULL,
  first_at    TEXT NOT NULL,
  last_at     TEXT NOT NULL,
  PRIMARY KEY (agent_id, fingerprint)
) STRICT`)
    },
  },
  {
    version: 4,
    description: 'add the project verification command table',
    apply: (db) => {
      db.exec("ALTER TABLE projects ADD COLUMN checks TEXT NOT NULL DEFAULT '{}'")
    },
  },
  {
    version: 5,
    description: 'add the consultation task flag',
    // A version-1 database is older than the tasks table itself, so the core
    // DDL has already created that table with the column; only a database that
    // predates this migration but already has the table needs the ALTER.
    apply: (db) => {
      const columns = db.prepare('PRAGMA table_info(tasks)').all() as Array<{ name: string }>
      if (columns.some(column => column.name === 'consultation')) return
      db.exec('ALTER TABLE tasks ADD COLUMN consultation INTEGER NOT NULL DEFAULT 0')
    },
  },
  {
    version: 6,
    description: 'record the review kind that produced a finding',
    apply: (db) => {
      const columns = db.prepare('PRAGMA table_info(findings)').all() as Array<{ name: string }>
      if (columns.some(column => column.name === 'review_kind')) return
      db.exec("ALTER TABLE findings ADD COLUMN review_kind TEXT NOT NULL DEFAULT 'code-review'")
    },
  },
  {
    version: 8,
    description: 'record the agents waiting for a leased resource',
    apply: (db) => {
      db.exec(`CREATE TABLE IF NOT EXISTS lease_requests (
  project_id    TEXT NOT NULL REFERENCES projects(id),
  resource_path TEXT NOT NULL,
  agent_id      TEXT NOT NULL,
  task_id       TEXT,
  mode          TEXT NOT NULL,
  reason        TEXT NOT NULL,
  requested_at  TEXT NOT NULL,
  PRIMARY KEY (project_id, resource_path, agent_id, mode)
) STRICT`)
    },
  },
  {
    version: 7,
    description: 'record every executed review run',
    apply: (db) => {
      db.exec(`CREATE TABLE IF NOT EXISTS review_runs (
  id                TEXT PRIMARY KEY,
  project_id        TEXT NOT NULL REFERENCES projects(id),
  task_id           TEXT NOT NULL,
  review_kind       TEXT NOT NULL,
  reviewer_agent_id TEXT NOT NULL,
  summary           TEXT NOT NULL,
  findings          INTEGER NOT NULL,
  rejected          INTEGER NOT NULL,
  created_at        TEXT NOT NULL
) STRICT`)
    },
  },
]

/**
 * Open a platform database and bring its schema to {@link NCAG_SCHEMA_VERSION}.
 * Missing parent directories are created; the database file itself is created
 * by SQLite. Migration runs inside one transaction so a failure leaves the
 * previous version intact.
 * @param path - database file path, or `:memory:` for an in-process database.
 * @returns the open handle with pragmas applied and the schema current.
 * @throws Error when the file is at a newer schema version than this build.
 */
export function openPlatformDatabase(path: string): DatabaseSync {
  const actual = path === ':memory:' ? path : resolve(path)
  if (actual !== ':memory:') mkdirSync(dirname(actual), { recursive: true })
  const db = new DatabaseSync(actual)
  try {
    db.exec('PRAGMA foreign_keys = ON')
    db.exec('PRAGMA journal_mode = WAL')
    db.exec('PRAGMA busy_timeout = 5000')
    const { user_version: onDisk } = db.prepare('PRAGMA user_version').get() as { user_version: number }
    if (onDisk > NCAG_SCHEMA_VERSION) {
      throw new Error(`ncag: database at ${JSON.stringify(actual)} has schema version ${onDisk}, newer than this build (${NCAG_SCHEMA_VERSION})`)
    }
    for (const statement of CORE_DDL) db.exec(statement)
    if (onDisk === 0) {
      stampFreshDatabase(db)
    } else if (onDisk < NCAG_SCHEMA_VERSION) {
      migrate(db, onDisk)
    }
    return db
  } catch (error: unknown) {
    db.close()
    throw error
  }
}

/** Record the fresh-database stamp and its schema history rows. */
function stampFreshDatabase(db: DatabaseSync): void {
  db.exec('BEGIN IMMEDIATE')
  try {
    const insert = db.prepare('INSERT OR REPLACE INTO schema_history (version, description, applied_at) VALUES (?, ?, ?)')
    insert.run(1, 'initial platform schema', new Date().toISOString())
    for (const migration of MIGRATIONS) {
      insert.run(migration.version, migration.description, new Date().toISOString())
    }
    db.exec(`PRAGMA user_version = ${NCAG_SCHEMA_VERSION}`)
    db.exec('COMMIT')
  } catch (error: unknown) {
    db.exec('ROLLBACK')
    throw error
  }
}

/** Apply the migrations an existing database still needs, in one transaction. */
function migrate(db: DatabaseSync, from: number): void {
  const pending = MIGRATIONS.filter(migration => migration.version > from)
  db.exec('BEGIN IMMEDIATE')
  try {
    const record = db.prepare('INSERT OR REPLACE INTO schema_history (version, description, applied_at) VALUES (?, ?, ?)')
    for (const migration of pending) {
      migration.apply(db)
      record.run(migration.version, migration.description, new Date().toISOString())
    }
    db.exec(`PRAGMA user_version = ${NCAG_SCHEMA_VERSION}`)
    db.exec('COMMIT')
  } catch (error: unknown) {
    db.exec('ROLLBACK')
    throw error
  }
}
