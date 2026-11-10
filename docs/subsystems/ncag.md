---
description: "The NC-AG subsystem: record types, the agent and task lifecycle state machines, communication and lease contracts, and the audit chain."
kind: "subsystem"
---

# NC-AG

English | [中文](ncag.zh.md)

This page declares the NC-AG platform types and lifecycles: the durable records every module exchanges, the two state machines, the message and lease contracts, and the audit chain. The orchestration behavior, deployment, operation and verification procedures are owned by the [NC-AG guides](../ncag/README.md).

Source: [`packages/ncag/ncag-core/src/types.ts`](../../packages/ncag/ncag-core/src/types.ts)

<a id="identifiers"></a>

## Identifiers

Every cross-module identifier is a `Branded<B>` string minted by `newId(kind)`, which returns the exact family brand through `IdFor`. `adoptId(kind, text)` re-establishes the brand for text read from the database and refuses a mismatched prefix, so a corrupt identifier fails at the read instead of propagating.

<a id="records"></a>

## Records

| Record | Identity | Durable state it owns |
|---|---|---|
`Project` | `ProjectId` | name, repository, base branch, status, autonomy policy, verification `checks`, shared context, usage |
`UserRequest` | `RequestId` | raw text, intent analysis, ambiguities, requirements, status |
`Requirement` | `RequirementId` | description, category, priority, source, dependencies, acceptance criteria, status |
`AgentProfile` | `ProfileId` | identity, personality, capabilities, tool names, permissions, restrictions, tier |
`AgentInstance` | `AgentId` | profile, project, task, status, workspace, branch, model, capabilities, usage, heartbeat |
`Task` | `TaskId` | capability, assigned agent, dependencies, inputs, outputs, criteria, reviews, verifications, status |
`NcagMessage` | `MessageId` | sender, target, type, content, correlation, priority, acknowledgement, delivery state |
`DecisionRecord` | `DecisionId` | category, problem, debate case, recommendation, outcome, rationale, consequences |
`ReviewFinding` | `FindingId` | severity, description, evidence, artifact, impact, correction, status |
`VerificationEvidence` | — | criterion, method, command, exit code, output, producing agent |
`ResourceLease` | `LeaseId` | path, mode, holder, task, fencing token, expiry, status |
`ChangeSet` | `ChangeSetId` | workspace, branch, base commit, head commit, changed files, status, report |
`AuditRecord` | `AuditId` | sequence, category, action, actor, correlation, detail, previous hash, hash |

<a id="state-machines"></a>

## State machines

```ts type-equiv
/** Every lifecycle state an agent instance can occupy. */
type AgentStatus =
  | 'CREATED'
  | 'INITIALIZING'
  | 'STARTING'
  | 'IDLE'
  | 'ASSIGNED'
  | 'WORKING'
  | 'WAITING'
  | 'BLOCKED'
  | 'HELP_REQUESTED'
  | 'REVIEWING'
  | 'HANDOFF_PENDING'
  | 'RECOVERING'
  | 'STOPPING'
  | 'STOPPED'
  | 'FAILED'
```

```ts type-equiv
/** Every lifecycle state a task can occupy. */
type TaskStatus =
  | 'CREATED'
  | 'ANALYZING'
  | 'PLANNED'
  | 'WAITING_APPROVAL'
  | 'READY'
  | 'ASSIGNED'
  | 'IN_PROGRESS'
  | 'BLOCKED'
  | 'REVIEW_PENDING'
  | 'VERIFICATION_PENDING'
  | 'COMPLETED'
  | 'REJECTED'
  | 'FAILED'
  | 'CANCELLED'
```

`transitionAgent` and `transitionTask` return the requested state or throw `NcagStateError` naming the machine, the current state and the requested state. `canTransitionAgent`, `canTransitionTask` and `allowedTransitions` answer without throwing. `STOPPED` and `COMPLETED` are terminal; `FAILED` re-enters work only through `RECOVERING` for an agent and `READY` for a task.

<a id="communication"></a>

## Communication contract

A message is recorded `PENDING`, then delivered, then acknowledged, with acknowledgement keyed by message and agent so a repeat does not double-count. Expiry is evaluated at delivery; a message that cannot be delivered within its lifetime is dead-lettered with the reason. Targets are an agent, a named channel or the orchestrator, so a project announcement is not an individual delivery.

<a id="leases"></a>

## Lease contract

A lease grants read or write over one repository-relative path to one agent for one task with a monotonic fencing token. A conflicting acquisition returns the owner, the owning task, the reason, the expiry and any alternative workspace. `canAct` refuses a lease that is released, expired or superseded by a newer token, which is what stops a slow agent from writing after another agent took the path.

<a id="audit"></a>

## Audit chain

Each record hashes its content together with the previous record hash, beginning at `AUDIT_GENESIS_HASH`. `verify()` walks the chain and reports the first sequence whose stored hash does not match, so a modified, deleted or inserted record is detectable from that point.

<!-- BEGIN GENERATED cordis-surface (gen-cordis-catalog.ts) — do not edit between markers -->

<a id="cordis-surface"></a>

## Cordis API

Generated from source by `scripts/gen-cordis-catalog.ts` (verified fresh by `bun run verify-cordis-catalog` in doc-sync; regenerate with `bun run gen-cordis-catalog`) — the language sides differ only in locale-specific paired document paths. Signature blocks use a `ts cordis-catalog` fence and keep the original source JSDoc; dispatch modes are defined in the [primer](../cordis-primer.md#dispatch-modes), and the framework-inherited `ctx` API lives in [cordis-api/inherited.md](../cordis-api/inherited.md).

<a id="ctxncag--ncagservice"></a>

### `ctx.ncag` — `NcagService`

The platform service. It creates the orchestrator once per database, hands it the harness model port, and disposes it when the last mount unloads.

```ts cordis-catalog
/**
 * Create one project anchored at the first registered workspace.
 * @param projectName - project name.
 * @param description - what the project delivers.
 * @returns the created project.
 * @throws Error when the harness has no workspace to anchor the project.
 */
createProjectFromWorkspace(projectName: string, description: string): Project
```

Source: [`packages/ncag/ncag-service/src/index.ts`](../../packages/ncag/ncag-service/src/index.ts)

<a id="ctxncagsettings--ncagsettings"></a>

### `ctx.ncagSettings` — `NcagSettings`

Host settings service for NC-AG.

```ts cordis-catalog
/**
 * Sample the current settings.
 * @returns effective overrides and limits.
 */
runtime(): NcagRuntimeSettings
```

Source: [`packages/ncag/ncag-settings/src/index.ts`](../../packages/ncag/ncag-settings/src/index.ts)
<!-- END GENERATED cordis-surface -->
