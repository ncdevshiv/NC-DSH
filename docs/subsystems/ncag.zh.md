---
description: "NC-AG 子系统：记录类型、智能体与任务生命周期状态机、通信与租约契约，以及审计链。"
kind: "subsystem"
---

# NC-AG

[English](ncag.md) | 中文

本页声明 NC-AG 平台的类型与生命周期：每个模块交换的持久记录、两个状态机、消息与租约契约，以及审计链。编排行为、部署、运维与验证流程由 [NC-AG 指南](../ncag/README.zh.md) 负责。

来源：[`packages/ncag/ncag-core/src/types.ts`](../../packages/ncag/ncag-core/src/types.ts)

<a id="identifiers"></a>

## 标识符

每个跨模块标识都是 `newId(kind)` 生成的 `Branded<B>` 字符串，`newId` 通过 `IdFor` 返回该族的确切品牌。`adoptId(kind, text)` 为从数据库读回的文本重建品牌，并拒绝前缀不匹配，因此损坏的标识符在读取时失败而不是继续传播。

<a id="records"></a>

## 记录

| 记录 | 标识 | 它拥有的持久状态 |
|---|---|---|
`Project` | `ProjectId` | 名称、仓库、基线分支、状态、自治策略、验证 `checks`、共享上下文、用量 |
`UserRequest` | `RequestId` | 原始文本、意图分析、歧义、需求、状态 |
`Requirement` | `RequirementId` | 描述、类别、优先级、来源、依赖、验收标准、状态 |
`AgentProfile` | `ProfileId` | 身份、性格、能力、工具名、权限、限制、层级 |
`AgentInstance` | `AgentId` | 档案、项目、任务、状态、工作区、分支、模型、能力、用量、心跳 |
`Task` | `TaskId` | 能力、已分配智能体、依赖、输入、产出、标准、审查、验证、状态 |
`NcagMessage` | `MessageId` | 发送者、目标、类型、内容、关联、优先级、确认、投递状态 |
`DecisionRecord` | `DecisionId` | 类别、问题、辩论记录、建议、结果、理由、后果 |
`ReviewFinding` | `FindingId` | 严重度、描述、证据、工件、影响、修正、状态 |
`VerificationEvidence` | — | 标准、方法、命令、退出码、输出、产生它的智能体 |
`ResourceLease` | `LeaseId` | 路径、模式、持有者、任务、围栏令牌、到期、状态 |
`ChangeSet` | `ChangeSetId` | 工作区、分支、基线提交、头提交、变更文件、状态、报告 |
`AuditRecord` | `AuditId` | 序号、类别、动作、执行者、关联、细节、前一哈希、哈希 |

<a id="state-machines"></a>

## 状态机

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

`transitionAgent` 与 `transitionTask` 返回请求的状态，或抛出 `NcagStateError`，其中指出状态机、当前状态与请求状态。`canTransitionAgent`、`canTransitionTask` 与 `allowedTransitions` 只回答而不抛出。`STOPPED` 与 `COMPLETED` 是终态；`FAILED` 仅能经 `RECOVERING`（智能体）或 `READY`（任务）重新进入工作。

<a id="communication"></a>

## 通信契约

消息先记录为 `PENDING`，再投递，再确认，确认以消息与智能体为键，因此重复不会重复计数。过期在投递时判定；在存活期内无法投递的消息带着原因进入死信。目标是智能体、具名通道或编排器，因此项目公告不是逐条单独投递。

<a id="leases"></a>

## 租约契约

租约把对一条仓库相对路径的读或写权利授予某智能体以完成某任务，并携带单调围栏令牌。冲突的获取返回持有者、所属任务、原因、到期时间与可能的替代工作区。`canAct` 拒绝已释放、已过期或已被更新令牌取代的租约，这正是阻止慢速智能体在另一智能体取得该路径后继续写入的原因。

<a id="audit"></a>

## 审计链

每条记录把自身内容与前一条记录哈希一起计算，起点为 `AUDIT_GENESIS_HASH`。`verify()` 遍历整条链并报告第一个存储哈希不匹配的序号，因此被修改、删除或插入的记录从该点起可被检测。

<!-- BEGIN GENERATED cordis-surface (gen-cordis-catalog.ts) — do not edit between markers -->

<a id="cordis-surface"></a>

## Cordis API

Generated from source by `scripts/gen-cordis-catalog.ts` (verified fresh by `bun run verify-cordis-catalog` in doc-sync; regenerate with `bun run gen-cordis-catalog`) — the language sides differ only in locale-specific paired document paths. Signature blocks use a `ts cordis-catalog` fence and keep the original source JSDoc; dispatch modes are defined in the [primer](../cordis-primer.zh.md#dispatch-modes), and the framework-inherited `ctx` API lives in [cordis-api/inherited.md](../cordis-api/inherited.md).

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
