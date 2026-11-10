---
description: "NC-AG 编排器：规划、DAG 调度、智能体生命周期、资源租约、Git 集成、审查、验证、决策、成本与恢复。"
kind: "package-library"
---

# @deepseek-ai/dsh-ncag-orchestrator

[English](README.md) | 中文

## 摘要

本包是监督智能。`NcagOrchestrator` 拥有一个平台存储、档案注册表与智能体实例、租约、消息日志、决策记录、成本账本、模型路由与恢复层。它规划请求、调度就绪任务、运行每个智能体、审查并验证结果、集成已批准变更，并把每一步记录到事件流与审计链。

每个模块都可独立使用：`planDeterministically`、DAG 辅助函数、`reviewTask`、`verifyTask`、`LeaseManager`、Git 辅助函数、`DecisionCenter`、`CostManager`、`withRetry` 与 `CircuitBreaker` 只接受显式输入，不保留隐藏的全局状态。

## 目录

- [编排器](#the-orchestrator)
- [规划](#planning)
- [调度](#scheduling)
- [执行](#execution)
- [审查与验证](#review-and-verification)
- [资源与 Git](#resources-and-git)
- [决策、成本与恢复](#decisions-cost-and-recovery)
- [已知限制与后续工作](#known-limitations-and-deferred-work)
- [开发者说明](#dev-note)

-----

<a id="the-orchestrator"></a>
## 编排器

| 方法 | 操作 |
|---|---|
| `createProject(input)` | 创建项目，确保其仓库有基线提交并记录 |
| `submitRequest({ projectId, rawText })` | 分析、规范化、分解、校验图并调度就绪工作 |
| `dispatch(project)` | 在并发与预算上限内调度就绪任务 |
| `snapshot()` | 控制平面渲染的运维快照 |
| `pauseProject` / `resumeProject` | 停止新调度 / 调度已持有的工作 |
| `cancelTask` | 中止智能体、释放其租约并记录取消 |
| `cancelAgent` | 取消一个活跃智能体、释放其租约并把任务返回队列 |
| `reassignTask` | 以另一种能力重新排队任务，并采用该档案的权限 |
| `resolveDecision` | 记录决策结果、应用其后果，无论接受还是覆盖都解除依赖任务阻塞 |
| `instructAgent` | 通过持久消息日志投递运维指令 |
| `observe()` | 健康巡检：检测并恢复停滞智能体 |
| `recoverAfterRestart()` | 恢复执行状态且不重复已完成工作 |
| `checkpoint(reason)` | 为每个项目持久化执行状态 |
| `start()` / `stop()` | 打开平台、解析路由、启动观察 / 记录检查点并关闭 |

<a id="planning"></a>
## 规划

`planDeterministically(requestText, projectName, capabilities, hasArchitecture, checks)` 产出 `ExecutionPlan`：意图分析（目标、动机、结果、约束、技术、范围、优先级、质量、安全）、含记录假设的歧义、含验收标准的需求，以及含能力、依赖、输入、产出、审查与验证方法的任务。`CAPABILITY_RULES` 把请求词汇映射到注册表能力；未命中任何规则的请求会变成一个通用实现任务。要求咨询某位专家的语句被归类为咨询：它变成一个专家任务，交付物是所记录的答复，由审查而非命令判定，并在完成时写入项目上下文。

<a id="scheduling"></a>
## 调度

`readyTasks` 选出处于 `PLANNED`/`READY`、依赖已终态且尚未分配智能体的任务，按优先级、期限、创建时间排序。`validateGraph` 在任何一个任务落库之前拒绝未知依赖、自依赖与任何环。`criticalPath` 报告最长依赖链，计划事件会记录它，使读者能看到决定工期的因素。

<a id="execution"></a>
## 执行

`runTask` 让一个任务走完生命周期：选择档案、解析路由、派生智能体、在写租约下创建工作树、以受策略约束的工具运行循环、审查、验证、集成、完成或失败。每个工具调用都由 `checkToolCall` 把关、记录为一次工具执行并做指纹计数，因此重复的相同调用会在耗尽预算前触发循环事件。用量按调用计价，并在每次记录后重新对照项目上限。

智能体可以在运行中通过运行时的两个协调工具进行协调。`answerHelp` 记录 `HELP_REQUEST`，在同一工作区上运行一个真正服务于所需能力的辅助智能体实例，记录 `HELP_RESPONSE` 并把答复返回请求方；`maxAgentsPerTask` 限定单个任务可使用的辅助智能体数量。`recordHandoff` 记录 `HANDOFF` 交接包，把任务转为接收方的能力，释放交接方，并让继任者在交接方的运行结束后从继承的工作区开始。被交接的任务绝不会再由交出它的智能体审查或集成。

<a id="review-and-verification"></a>
## 审查与验证

`reviewTask(input, delegate)` 检视变更文件、工具执行、验证结果与完成报告，提出带证据的按严重度排序的发现，并可选地请被委派的模型审查者基于同一证据给出第二意见。`verifyTask` 执行每个标准并记录命令、退出码与输出；`unverifiedCriteria` 决定是否完成。

<a id="resources-and-git"></a>
## 资源与 Git

`LeaseManager` 授予带单调围栏令牌的读或写权利，拒绝冲突持有者并返回冲突细节，以过期与撤销代替删除，并以围栏检查回答 `canAct`。Git 辅助函数为每个智能体创建独立工作树、在提交时提交、列出变更文件、无快进合并、验证整合后的树、在验证失败时回滚，并在文本冲突时不触碰基线分支。

<a id="decisions-cost-and-recovery"></a>
## 决策、成本与恢复

`DecisionCenter.debate()` 拒绝不完整的异议；`propose` 对自主决策立即记录，对重大决策等待用户；`decide` 记录结果、理由与被拒绝的建议。`CostManager` 按路由价格计价每次调用，并以剩余额度回答 `budget`。`withRetry` 在结果未知时拒绝重复非幂等操作，`backoffDelay` 限制重试节奏，`CircuitBreaker` 停止调用持续失败的提供方，`detectStall` 把心跳丢失、进度丢失与重复调用分别报告为证据。

<a id="known-limitations-and-deferred-work"></a>
## 已知限制与后续工作

- **每个数据库一个编排器。** 恢复只为一个进程恢复状态；多个编排器共用一个平台数据库不受支持。
- **语义冲突解决是记录而非自动化。** 集成管理器检测重叠与验证失败；两个智能体之间的语义分歧通过决策解决，而不是通过自动合并启发式。
- **委派审查需要路由。** 没有委派端口时，审查只基于已记录证据进行，并报告未获得第二意见；它绝不虚构一个意见。

<a id="dev-note"></a>
## 开发者说明

<details>
<summary>维护者工作背景 —— 点击展开</summary>

编排器不持有模块级状态：它保存的每个映射（中止控制器、停滞智能体、租约链）都是实例字段，持久真相位于存储中。这正是重启安全、测试可以并行运行两个编排器的原因。

</details>
