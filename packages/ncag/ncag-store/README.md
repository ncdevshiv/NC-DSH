---
description: "The NC-AG durable store: SQLite schema and migrations, typed repositories, the platform event stream, and the hash-chained audit log."
kind: "package-library"
---

# @deepseek-ai/dsh-ncag-store

English | [中文](README.zh.md)

## Summary

The platform database: one SQLite file holding every project, request, requirement, profile, agent, task, message, decision, finding, evidence row, lease, change set, workspace, tool execution, usage row, checkpoint and audit record. `NcagStore` owns the connection, the ordered event stream and the audit chain; `createRepositories(store.db)` returns the typed repositories that read and write the rows.

## Table of Contents

- [Use this package](#use-this-package)
- [Schema and migrations](#schema-and-migrations)
- [Repositories](#repositories)
- [Events and audit](#events-and-audit)
- [Known Limitations and Deferred Work](#known-limitations-and-deferred-work)
- [Dev Note](#dev-note)

-----

<a id="use-this-package"></a>
## Use this package

```ts
const store = NcagStore.open('/abs/path/platform.db')
const repositories = createRepositories(store.db)
repositories.projects.save(project)
store.emitEvent({ kind: 'project/created', projectId: project.id, summary: 'project created' })
store.recordAudit({ category: 'request', action: 'project-created', actor: 'orchestrator', correlationId, detail: {} })
store.close()
```

`close()` is idempotent. Opening creates missing directories and sets `journal_mode = WAL`, `foreign_keys = ON` and `busy_timeout = 5000`.

<a id="schema-and-migrations"></a>
## Schema and migrations

The schema version lives in `PRAGMA user_version`. A fresh database is created complete and stamped; an older database runs the ordered `MIGRATIONS` list inside one transaction, so a failed migration leaves the previous version intact; a database newer than the build is refused rather than downgraded. Every migration is recorded in `schema_history`.

JSON-valued columns are stored as text, which keeps the physical schema inspectable with plain SQLite tooling. Values are decoded through `decodeJson`, which refuses a null column that has no documented fallback.

<a id="repositories"></a>
## Repositories

| Repository | Records |
|---|---|
| `projects`, `requests`, `requirements` | the request side: project configuration, the raw request, normalized requirements |
| `profiles`, `agents` | the specialization registry and spawned instances |
| `tasks` | the task graph with status, dependencies, criteria and attempts |
| `messages` | the durable communication log with delivery and acknowledgement state |
| `decisions`, `findings`, `evidence` | decisions and their debate, review findings, verification evidence |
| `leases`, `changeSets`, `workspaces` | resource ownership, submitted changes, agent worktrees |
| `toolCalls`, `usage`, `checkpoints`, `toolInvocations` | execution records, cost ledger, recovery checkpoints, loop counters |

`addUsage` sums usage totals, and `EMPTY_USAGE` / `DEFAULT_AUTONOMY` are the documented defaults used when a record has none.

<a id="events-and-audit"></a>
## Events and audit

`emitEvent(input)` appends one ordered platform event and notifies subscribers; `subscribe` returns its disposer, and `recentEvents(limit, projectId?)` reads newest events oldest-first. The stream is what the control plane forwards to the dashboard.

`AuditLog` appends a record whose hash covers its content and the previous record's hash, starting from `AUDIT_GENESIS_HASH`. `verify()` walks the chain and reports the first sequence that does not match, so a modified, deleted or inserted record is detectable from that point on. A verified chain is the platform's integrity claim.

<a id="known-limitations-and-deferred-work"></a>
## Known Limitations and Deferred Work

- **One connection per process.** The orchestrator owns it and the repositories share it; a second process opening the same file is protected by WAL and `busy_timeout`, not by a lease. Running several orchestrators over one database is not a supported deployment.
- **No compaction.** Events, evidence and tool executions grow without bound. Retention is the operator's job today; a retention policy belongs here once a deployment needs one.

<a id="dev-note"></a>
## Dev Note

<details>
<summary>Working context for maintainers — click to expand</summary>

Statements live in `sql.ts` so a column change is edited beside its DDL. `asRows<T>` is the single place that adopts a prepared statement's generic result as the caller's row interface; a new query should use it rather than casting at the call site.

</details>
