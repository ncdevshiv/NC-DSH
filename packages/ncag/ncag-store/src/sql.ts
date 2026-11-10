/**
 * The SQL statements the repositories execute, kept in one module so a column
 * change is edited beside its DDL and every reader sees the same text.
 * @module @deepseek-ai/dsh-ncag-store/sql
 */

export const INSERT_PROJECT = 'INSERT INTO projects (id, name, description, repository_path, base_branch, status, instructions, autonomy, context, checks, usage, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT(id) DO UPDATE SET name = excluded.name, description = excluded.description, repository_path = excluded.repository_path, base_branch = excluded.base_branch, status = excluded.status, instructions = excluded.instructions, autonomy = excluded.autonomy, context = excluded.context, checks = excluded.checks, usage = excluded.usage, updated_at = excluded.updated_at'
export const SELECT_PROJECT = 'SELECT * FROM projects WHERE id = ?'
export const SELECT_ALL_PROJECTS = 'SELECT * FROM projects ORDER BY created_at DESC'

export const INSERT_REQUEST = 'INSERT INTO requests (id, project_id, raw_text, created_at, intent, ambiguities, status) VALUES (?, ?, ?, ?, ?, ?, ?) ON CONFLICT(id) DO UPDATE SET intent = excluded.intent, ambiguities = excluded.ambiguities, status = excluded.status'
export const SELECT_REQUEST = 'SELECT * FROM requests WHERE id = ?'
export const SELECT_PROJECT_REQUESTS = 'SELECT * FROM requests WHERE project_id = ? ORDER BY created_at DESC'
export const SELECT_REQUEST_IDS = 'SELECT id FROM requirements WHERE request_id = ?'
export const DELETE_REQUIREMENT = 'DELETE FROM requirements WHERE id = ?'
export const INSERT_REQUIREMENT = 'INSERT INTO requirements (id, request_id, project_id, description, category, priority, source, dependencies, acceptance, status) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT(id) DO UPDATE SET description = excluded.description, category = excluded.category, priority = excluded.priority, dependencies = excluded.dependencies, acceptance = excluded.acceptance, status = excluded.status'
export const SELECT_REQUEST_REQUIREMENTS = 'SELECT * FROM requirements WHERE request_id = ? ORDER BY rowid'
export const SELECT_PROJECT_REQUIREMENTS = 'SELECT * FROM requirements WHERE project_id = ? ORDER BY rowid'

export const INSERT_PROFILE = 'INSERT INTO profiles (id, key, payload, built_in) VALUES (?, ?, ?, ?) ON CONFLICT(id) DO UPDATE SET payload = excluded.payload, built_in = excluded.built_in'
export const SELECT_PROFILE = 'SELECT * FROM profiles WHERE id = ?'
export const SELECT_PROFILE_BY_KEY = 'SELECT * FROM profiles WHERE key = ?'
export const SELECT_ALL_PROFILES = 'SELECT * FROM profiles ORDER BY built_in DESC, key ASC'
export const DELETE_PROFILE = 'DELETE FROM profiles WHERE id = ?'

export const AGENT_COLUMNS = 'id, profile_id, profile_key, project_id, task_id, status, status_reason, workspace_path, branch, model, capabilities, usage, failure_count, steps, active_correlation, created_at, updated_at, last_heartbeat_at, last_activity_at, stopped_at'
export const INSERT_AGENT = 'INSERT INTO agents (' + AGENT_COLUMNS + ') VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT(id) DO UPDATE SET task_id = excluded.task_id, status = excluded.status, status_reason = excluded.status_reason, workspace_path = excluded.workspace_path, branch = excluded.branch, model = excluded.model, capabilities = excluded.capabilities, usage = excluded.usage, failure_count = excluded.failure_count, steps = excluded.steps, active_correlation = excluded.active_correlation, updated_at = excluded.updated_at, last_heartbeat_at = excluded.last_heartbeat_at, last_activity_at = excluded.last_activity_at, stopped_at = excluded.stopped_at'
export const SELECT_AGENT = 'SELECT * FROM agents WHERE id = ?'
export const SELECT_PROJECT_AGENTS = 'SELECT * FROM agents WHERE project_id = ? ORDER BY created_at ASC'
export const SELECT_LIVE_AGENTS = 'SELECT * FROM agents WHERE status NOT IN (?, ?) ORDER BY created_at ASC'
export const DELETE_AGENT = 'DELETE FROM agents WHERE id = ?'

export const TASK_COLUMNS = 'id, project_id, request_id, requirement_ids, title, description, required_capability, assigned_agent_id, dependency_ids, required_inputs, expected_outputs, acceptance_criteria, required_permissions, context_refs, priority, deadline_ms, required_reviews, required_verifications, status, status_reason, depth, attempts, review_cycles, consultation, created_at, updated_at, started_at, completed_at'
export const INSERT_TASK = 'INSERT INTO tasks (' + TASK_COLUMNS + ') VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT(id) DO UPDATE SET assigned_agent_id = excluded.assigned_agent_id, status = excluded.status, status_reason = excluded.status_reason, priority = excluded.priority, deadline_ms = excluded.deadline_ms, depth = excluded.depth, attempts = excluded.attempts, review_cycles = excluded.review_cycles, consultation = excluded.consultation, required_capability = excluded.required_capability, context_refs = excluded.context_refs, required_permissions = excluded.required_permissions, updated_at = excluded.updated_at, started_at = excluded.started_at, completed_at = excluded.completed_at'
export const SELECT_TASK = 'SELECT * FROM tasks WHERE id = ?'
export const SELECT_PROJECT_TASKS = 'SELECT * FROM tasks WHERE project_id = ? ORDER BY depth ASC, created_at ASC'
export const DELETE_TASK = 'DELETE FROM tasks WHERE id = ?'

export const INSERT_MESSAGE = 'INSERT INTO messages (id, at, sender, target, project_id, task_id, type, subject, content, context_refs, correlation_id, priority, requires_ack, delivery_status, delivered_at, acknowledged_at, attempts, expires_at_ms, dead_letter_reason) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)'
export const SELECT_MESSAGE = 'SELECT * FROM messages WHERE id = ?'
export const SELECT_PROJECT_MESSAGES = 'SELECT * FROM messages WHERE project_id = ? ORDER BY rowid ASC'
export const SELECT_UNDELIVERED = 'SELECT * FROM messages WHERE delivery_status = ? ORDER BY rowid ASC LIMIT ?'
export const UPDATE_MESSAGE_DELIVERY = 'UPDATE messages SET delivery_status = ?, delivered_at = ?, acknowledged_at = ?, attempts = ?, dead_letter_reason = ? WHERE id = ?'
export const ACK_MESSAGE = 'INSERT INTO messages_acks (message_id, agent_id, at) VALUES (?, ?, ?) ON CONFLICT(message_id, agent_id) DO UPDATE SET at = excluded.at'
export const SELECT_MESSAGE_ACK = 'SELECT 1 AS ok FROM messages_acks WHERE message_id = ? AND agent_id = ?'
export const UPSERT_CURSOR = 'INSERT INTO message_cursors (agent_id, last_seen, updated_at) VALUES (?, ?, ?) ON CONFLICT(agent_id) DO UPDATE SET last_seen = excluded.last_seen, updated_at = excluded.updated_at'
export const SELECT_CURSOR = 'SELECT last_seen FROM message_cursors WHERE agent_id = ?'

export const INSERT_DECISION = 'INSERT INTO decisions (id, project_id, task_id, category, author, problem, debate, recommendation, outcome, rationale, rejected_recommendation, consequences, affected_modules, affected_task_ids, status, review_history, created_at, decided_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT(id) DO UPDATE SET outcome = excluded.outcome, rationale = excluded.rationale, rejected_recommendation = excluded.rejected_recommendation, status = excluded.status, review_history = excluded.review_history, decided_at = excluded.decided_at, consequences = excluded.consequences'
export const SELECT_DECISION = 'SELECT * FROM decisions WHERE id = ?'
export const SELECT_PROJECT_DECISIONS = 'SELECT * FROM decisions WHERE project_id = ? ORDER BY rowid ASC'
export const SELECT_PENDING_DECISIONS = 'SELECT * FROM decisions WHERE status = ? ORDER BY rowid ASC'

export const INSERT_FINDING = 'INSERT INTO findings (id, project_id, task_id, reviewer_agent_id, review_kind, severity, description, evidence, artifact, impact, recommendation, verification, status, created_at, resolved_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT(id) DO UPDATE SET status = excluded.status, resolved_at = excluded.resolved_at'
export const SELECT_TASK_FINDINGS = 'SELECT * FROM findings WHERE task_id = ? ORDER BY rowid ASC'
export const SELECT_OPEN_FINDINGS = 'SELECT * FROM findings WHERE status = ? ORDER BY rowid ASC'

export const INSERT_REVIEW_RUN = 'INSERT INTO review_runs (id, project_id, task_id, review_kind, reviewer_agent_id, summary, findings, rejected, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)'
export const SELECT_TASK_REVIEW_RUNS = 'SELECT * FROM review_runs WHERE task_id = ? ORDER BY rowid ASC'
export const SELECT_PROJECT_REVIEW_RUNS = 'SELECT * FROM review_runs WHERE project_id = ? ORDER BY rowid ASC'

export const INSERT_EVIDENCE = 'INSERT INTO evidence (task_id, project_id, criterion_id, method, status, command, exit_code, output, recorded_at, produced_by) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)'
export const SELECT_TASK_EVIDENCE = 'SELECT * FROM evidence WHERE task_id = ? ORDER BY sequence ASC'

export const INSERT_LEASE = 'INSERT INTO leases (id, project_id, resource_path, mode, agent_id, task_id, fencing_token, acquired_at_ms, renewed_at_ms, expires_at_ms, status, reason) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)'
export const SELECT_LEASE = 'SELECT * FROM leases WHERE id = ?'
export const SELECT_ACTIVE_LEASES = 'SELECT * FROM leases WHERE project_id = ? AND status = ? ORDER BY resource_path ASC, acquired_at_ms ASC'
export const UPDATE_LEASE = 'UPDATE leases SET status = ?, renewed_at_ms = ?, expires_at_ms = ?, fencing_token = ? WHERE id = ?'
export const SELECT_MAX_FENCING = 'SELECT COALESCE(MAX(fencing_token), 0) AS token FROM leases WHERE project_id = ?'
export const DELETE_LEASES_FOR_AGENT = 'UPDATE leases SET status = ? WHERE agent_id = ? AND status = ?'
export const RECORD_LEASE_REQUEST = 'INSERT INTO lease_requests (project_id, resource_path, agent_id, task_id, mode, reason, requested_at) VALUES (?, ?, ?, ?, ?, ?, ?) ON CONFLICT(project_id, resource_path, agent_id, mode) DO UPDATE SET reason = excluded.reason, task_id = excluded.task_id, requested_at = excluded.requested_at'
export const SELECT_LEASE_WAITERS = 'SELECT * FROM lease_requests WHERE project_id = ? AND resource_path = ? AND mode = ? ORDER BY requested_at ASC'
export const CLEAR_LEASE_REQUEST = 'DELETE FROM lease_requests WHERE project_id = ? AND resource_path = ? AND agent_id = ? AND mode = ?'
export const SELECT_PROJECT_LEASE_WAITERS = 'SELECT * FROM lease_requests WHERE project_id = ? ORDER BY requested_at ASC'

export const INSERT_CHANGESET = 'INSERT INTO changesets (id, project_id, task_id, agent_id, workspace_path, branch, base_commit, head_commit, changed_files, message, status, submitted_at, report, integrated_commit) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT(id) DO UPDATE SET head_commit = excluded.head_commit, changed_files = excluded.changed_files, status = excluded.status, report = excluded.report, integrated_commit = excluded.integrated_commit'
export const SELECT_CHANGESET = 'SELECT * FROM changesets WHERE id = ?'
export const SELECT_PROJECT_CHANGESETS = 'SELECT * FROM changesets WHERE project_id = ? ORDER BY submitted_at ASC'
export const SELECT_OPEN_CHANGESETS = 'SELECT * FROM changesets WHERE project_id = ? AND status IN (?, ?) ORDER BY submitted_at ASC'

export const INSERT_WORKSPACE = 'INSERT INTO workspaces (agent_id, project_id, task_id, path, branch, base_commit, created_at, released_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT(agent_id) DO UPDATE SET task_id = excluded.task_id, path = excluded.path, branch = excluded.branch, base_commit = excluded.base_commit, released_at = excluded.released_at'
export const SELECT_WORKSPACE = 'SELECT * FROM workspaces WHERE agent_id = ?'
export const RELEASE_WORKSPACE = 'UPDATE workspaces SET released_at = ? WHERE agent_id = ?'

export const INSERT_AUDIT = 'INSERT INTO audit (id, at, category, action, actor, project_id, task_id, agent_id, correlation_id, detail, previous_hash, hash) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)'
export const SELECT_AUDIT_TAIL = 'SELECT * FROM audit ORDER BY sequence DESC LIMIT ?'
export const SELECT_AUDIT_ALL = 'SELECT * FROM audit ORDER BY sequence ASC'
export const SELECT_AUDIT_TIP = 'SELECT sequence, hash FROM audit ORDER BY sequence DESC LIMIT 1'

export const INSERT_EVENT = 'INSERT INTO platform_events (at, kind, project_id, task_id, agent_id, summary, data) VALUES (?, ?, ?, ?, ?, ?, ?)'
export const SELECT_LAST_ROWID = 'SELECT last_insert_rowid() AS seq'
export const SELECT_EVENTS = 'SELECT * FROM platform_events ORDER BY sequence DESC LIMIT ?'
export const SELECT_PROJECT_EVENTS = 'SELECT * FROM platform_events WHERE project_id = ? ORDER BY sequence DESC LIMIT ?'

export const INSERT_TOOL_CALL = 'INSERT INTO tool_calls (at, agent_id, project_id, task_id, tool, args, ok, error, output, duration_ms, correlation_id) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)'
export const SELECT_TOOL_CALLS = 'SELECT * FROM tool_calls WHERE agent_id = ? ORDER BY sequence DESC LIMIT ?'

export const INSERT_USAGE = 'INSERT INTO usage_ledger (at, project_id, task_id, agent_id, provider, model, input_tokens, output_tokens, cached_tokens, cost_usd, purpose) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)'
export const SELECT_PROJECT_USAGE = 'SELECT COALESCE(SUM(input_tokens), 0) AS input_tokens, COALESCE(SUM(output_tokens), 0) AS output_tokens, COALESCE(SUM(cached_tokens), 0) AS cached_tokens, COALESCE(SUM(cost_usd), 0) AS cost_usd, COUNT(*) AS calls FROM usage_ledger WHERE project_id = ?'

export const INSERT_CHECKPOINT = 'INSERT INTO checkpoints (id, project_id, at, reason, state) VALUES (?, ?, ?, ?, ?)'
export const SELECT_CHECKPOINTS = 'SELECT * FROM checkpoints WHERE project_id = ? ORDER BY at ASC'

export const UPSERT_TOOL_INVOCATION = 'INSERT INTO tool_invocations (agent_id, fingerprint, count, first_at, last_at) VALUES (?, ?, 1, ?, ?) ON CONFLICT(agent_id, fingerprint) DO UPDATE SET count = count + 1, last_at = excluded.last_at'
export const SELECT_TOOL_INVOCATION = 'SELECT count, last_at FROM tool_invocations WHERE agent_id = ? AND fingerprint = ?'
export const RESET_TOOL_INVOCATIONS = 'DELETE FROM tool_invocations WHERE agent_id = ?'

export const INSERT_SCHEMA_HISTORY = 'INSERT OR REPLACE INTO schema_history (version, description, applied_at) VALUES (?, ?, ?)'
