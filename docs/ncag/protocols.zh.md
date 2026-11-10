---
description: "NC-AG 协议：智能体与任务生命周期状态机、消息类型与投递、资源租约协议、决策与 Git 集成。"
kind: "reference"
---

# NC-AG 协议

[English](protocols.md) | 中文

## 摘要

本页定义每个 NC-AG 模块遵守的状态机与协议。可执行定义位于 [`ncag-core`](../../packages/ncag/ncag-core/README.zh.md) 以及各协议旁注明的执行模块；改动生命周期必须先改该包。

## 目录

- [智能体生命周期](#agent-lifecycle)
- [任务生命周期](#task-lifecycle)
- [通信](#communication)
- [资源租约](#resource-leases)
- [Git 工作区与集成](#git-workspaces-and-integration)
- [决策与辩论](#decisions-and-debate)
- [审查与验证](#review-and-verification)

-----

<a id="agent-lifecycle"></a>
## 智能体生命周期

`CREATED → INITIALIZING → STARTING → IDLE ⇄ ASSIGNED ⇄ WORKING`，分支状态为 `WAITING`、`BLOCKED`、`HELP_REQUESTED`、`REVIEWING`、`HANDOFF_PENDING`、`RECOVERING`，终态路径为 `STOPPING → STOPPED` 或 `FAILED`。

`FAILED` 是唯一可经 `RECOVERING` 返回工作的状态。`STOPPED` 及其出边不存在。[`transitionAgent`](../../packages/ncag/ncag-core/src/machines.ts) 拒绝表外的一切迁移，而 [`AgentRegistry.transition`](../../packages/ncag/ncag-orchestrator/src/agents.ts) 是智能体状态的唯一写入者，因此没有模块能把智能体移动到状态机不允许的状态。

心跳记录观察，而非进度。`heartbeat(agentId, producedProgress)` 仅在智能体产生持久进度（工具结果）时推进 `lastActivityAt`，这正是停滞检测读取的字段。

<a id="task-lifecycle"></a>
## 任务生命周期

`CREATED → ANALYZING → PLANNED → WAITING_APPROVAL → READY → ASSIGNED → IN_PROGRESS → REVIEW_PENDING → VERIFICATION_PENDING → COMPLETED`，另有 `BLOCKED`、`REJECTED`、`FAILED`、`CANCELLED` 作为替代路径。

`COMPLETED` 与 `CANCELLED` 是终态。`REJECTED` 可返回 `IN_PROGRESS`（审查者退回），`FAILED` 可返回 `READY`（恢复管理器重新分配）。只有依赖已终态且未分配智能体的 `PLANNED` 与 `READY` 任务可被调度。

<a id="communication"></a>
## 通信

每条消息携带标识、UTC 时间戳、发送者（智能体或编排器的 null）、目标、项目、任务、类型、主题、内容、上下文引用、关联标识、优先级、确认要求与投递状态。

类型：`INFORMATION`、`TASK_UPDATE`、`HELP_REQUEST`、`HELP_RESPONSE`、`BLOCKER`、`RESOURCE_REQUEST`、`RESOURCE_RELEASE`、`CONTRACT_PROPOSAL`、`CONTRACT_CHANGE`、`ARCHITECTURE_PROPOSAL`、`ARCHITECTURE_CONFLICT`、`REVIEW_REQUEST`、`REVIEW_FINDING`、`HANDOFF`、`ESCALATION`、`TASK_COMPLETION`、`FAILURE`、`SYSTEM_EVENT`。

投递状态为 `PENDING → DELIVERED → ACKNOWLEDGED`，另有 `EXPIRED` 与 `DEAD_LETTERED`。消息不会因为发送者尝试过就算投递：存储先记录 `PENDING`，投递泵推进它，确认是按消息与智能体为键的独立记录，因此重复确认不会被重复计数。过期在投递时判定，在存活期内无法投递的消息带着原因进入死信。

目标是智能体、具名通道或编排器。通道存在，是为了让项目级公告不会变成逐条单独投递。

智能体通过它持有的两个运行时工具发起协调请求：`request_help` 发送 `HELP_REQUEST`，编排器通过运行一个真正服务于所需能力的辅助智能体实例来作答，把辅助答复返回给请求方并记录为 `HELP_RESPONSE`。`hand_off` 发送 `HANDOFF` 交接包，并把任务以接收方的能力重新排队；继任者继承交接方的工作区，因此同一条已变更的树得以延续。两个工具都在 `communication:send` 权限下运行，项目的 `maxAgentsPerTask` 限定单个任务可使用的辅助智能体数量，交接方在自己的运行结束后停止，而不会去审查一份它已不再拥有的交付。

<a id="resource-leases"></a>
## 资源租约

租约把对一条仓库相对路径的读或写权利授予某智能体以完成某任务，并携带单调围栏令牌。写租约互斥；设置 `allowSharedRead` 时读租约可共享。

当活跃写租约或任何活跃读租约持有该路径时，获取被拒绝，并返回冲突，指出持有者、所属任务、声明的原因、到期时间以及是否存在替代工作区。续约拒绝已被取代的租约，因为它携带的围栏令牌不再是最新的。`canAct` 是执行者在写入前所做的检查：已过期、已释放或已被取代的租约意味着智能体不得行动。

租约过期与强制释放被记录为迁移（`EXPIRED`、`REVOKED`）而非静默删除，每个智能体都保留其持有内容的证据。

被拒绝的获取同样是持久的。等待中的智能体、其任务、模式与声明的原因被记录到 `lease_requests`，每个项目、路径、智能体与模式一行，因此重复请求保持其原有队列位置。授予会清除该智能体的条目；运维者通过 `LeaseManager.waiting(projectId)` 读取整个项目队列，这正是控制平面的 waiters 路由所提供的内容。队列在重启后依然存在，因为它是表，而不是保存在内存中的 promise。

<a id="git-workspaces-and-integration"></a>
## Git 工作区与集成

每个智能体在自己的工作树、自己的分支上工作，从项目基线分支创建，并记录基线提交。提交会提交工作区、相对基线列出变更文件并记录变更集。

集成签出基线分支，无快进合并提交分支，随后运行配置的验证。文本合并成功并不证明集成正确：由验证判定，验证失败会回滚合并并报告。文本冲突会中止合并、记录冲突并保持基线分支不变。

<a id="decisions-and-debate"></a>
## 决策与辩论

决策记录携带：标识、项目、任务、类别、作者、问题、辩论记录、建议、结果、理由、被拒绝的建议、后果、受影响模块与任务、状态、创建时间、决策时间与复核历史。

四个类别：`autonomous`（编排器自行决定并立即记录）、`technical-recommendation`（默认自行决定，可升级）、`material-architectural`（等待用户）、`user-reserved`（等待用户）。

`DecisionCenter.debate()` 拒绝不完整的记录：必须陈述请求内容、平台主张、确切冲突、建议与问题，以同一组维度比较至少两个备选方案，并给出证据。这正是决策中心总能显示真实异议而不是单方通知的原因。

记录覆盖会把用户选择写入项目约束、记录被拒绝的建议与理由、解除等待该决策的任务、在项目通道公告该决策并继续。除非有新信息，平台不会重开已定决策。

<a id="review-and-verification"></a>
## 审查与验证

审查检视已记录证据，而非智能体的断言：变更文件、工具执行与验证结果。没有变更文件、执行失败、标准未验证或完成报告为空的审查会产生一条发现，含严重度、描述、证据、工件、影响、建议修正与验证方法。`CRITICAL` 与 `HIGH` 使任务被拒绝。咨询任务以书面答复交付，因此没有变更集并不构成缺陷：它以答复本身接受审查，并写入项目上下文。

验证执行每个标准所指的命令——标准自身的 `command`，或项目为其方法配置的检查——并记录确切命令、退出码与捕获的输出。完成由已记录证据计算：任何没有通过证据的标准都会让任务无法进入 `COMPLETED`。

没有配置命令的 `manual-review` 标准改由已执行的审查判定：仅当该任务确有审查运行、且其中没有 `CRITICAL` 或 `HIGH` 发现时通过；审查拒绝时以发现列表失败。没有命令也没有审查证据的方法会带着原因失败，因为该检查无法执行。

审查拒绝会消耗一个审查周期并记录在任务上。超过 `maxReviewCycles` 的任务停止返工并以失败结束，同时发出携带全部未决发现的 `ESCALATION` 消息，因此被拒绝的交付不会无限循环。
