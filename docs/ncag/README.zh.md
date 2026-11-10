---
description: "NC-AG：构建在 harness 内的自主多智能体编排平台 —— 架构、部署、运维与验证。"
kind: "subsystem"
---

# NC-AG

[English](README.md) | 中文

## 摘要

NC-AG 是一个自主多智能体编排平台：它接收复杂指令，分析意图，质疑矛盾之处，把工作分解为任务图，在相互隔离的 Git 工作树中派生专职智能体，协调它们的通信与资源占用，审查并独立验证它们的产出，交付一份有完整证据链支撑的结果。本页是入口：先给出架构地图，再指向部署、运维、安全与验证指南。

## 目录

- [NC-AG 做什么](#what-ncag-does)
- [架构](#architecture)
- [运行](#run-it)
- [指南](#guides)
- [NC-AG 不会做什么](#what-ncag-will-not-do)

-----

<a id="what-ncag-does"></a>
## NC-AG 做什么

请求通过 `ncag_request`（一个 harness 工具）或控制平面 API 进入。平台随后：

1. **分析**请求的意图、约束、技术、质量期望与安全影响，并记录每一处歧义。重大歧义上升为决策；非重大歧义成为已记录假设。
2. **规范化**需求，赋予标识符、优先级、验收标准与验证方法，并追溯到产生它的请求语句。
3. **分解**为任务，每个任务含能力、依赖、输入、预期产出、审查种类与验证方法，随后校验依赖图（无未知依赖、无环）。
4. **调度**就绪任务，受项目并发上限与关键路径约束。
5. **派生**一个智能体实例执行任务，起于已注册档案，位于其自有 Git 工作树，并以带围栏令牌的写租约保护。
6. **执行**智能体：循环询问模型下一步动作，在权限、越界与破坏性命令策略下运行模型请求的工具，并回填结果。
7. **审查**：由独立审查者检视已记录证据；发现按严重度排序并记录，CRITICAL 或 HIGH 发现使任务被拒绝。
8. **验证**每个验收标准，执行项目配置的检查命令，并把确切命令、退出码与输出记录为证据。
9. **集成**已批准的变更，经集成管理器记录变更文件、合并结果及其验证报告。
10. **交付**仅在每个标准都有通过证据时发生；失败的检查永远保持可见。

<a id="architecture"></a>
## 架构

平台是一组具有显式接口的包。[`ncag-core`](../../packages/ncag/ncag-core/README.zh.md) 拥有词汇与生命周期状态机，因此没有模块能臆造一次状态迁移。[`ncag-store`](../../packages/ncag/ncag-store/README.zh.md) 是持久状态的唯一写入者：一个 SQLite 数据库、类型化仓储、有序事件流与哈希链审计日志。[`ncag-runtime`](../../packages/ncag/ncag-runtime/README.zh.md) 针对模型端口与工作区执行一个智能体，把模型调用与工具执行置于接口之后。[`ncag-orchestrator`](../../packages/ncag/ncag-orchestrator/README.zh.md) 做出决策：规划、调度、监督、租约、集成、审查、验证、决策、成本与恢复。[`ncag-dashboard`](../../packages/ncag/ncag-dashboard/README.zh.md) 通过 HTTP 暴露平台并提供运维 UI。[`tool-ncag`](../../packages/ncag/tool-ncag/README.zh.md) 在 harness 中挂载平台并注册面向模型的工具。

编排器做决策；确定性基础设施执行这些决策。重启编排器会从数据库恢复每个任务、消息、租约、决策与事件；重启绝不会静默丢失对进行中工作的追踪，已完成任务也不会被重复执行。

<a id="run-it"></a>
## 运行

```bash
# 1. Required configuration
export NCAG_DATABASE_PATH="$HOME/ncag/platform.db"
export NCAG_WORKSPACE_ROOT="$HOME/ncag/workspaces"
export NCAG_MODEL_PRICING='[{"provider":"deepseek","model":"deepseek-chat","inputPerMillion":0.27,"outputPerMillion":1.1}]'

# 2. Mount the platform beside the harness
dsh --profile web --patch packages/bundle/ncag/cordis.patch.yml
```

`ncag` 档案用一个名称完成同样的组合：`dsh --profile ncag web`。控制平面是由 [`ncag-dashboard`](../../packages/ncag/ncag-dashboard/README.zh.md) 启动的进程内 HTTP 服务，而不是独立可执行文件。

随发行版交付的 Web 应用同时以 **NC-AG 模式** Agent preset 提供该平台。在 Agent 选择器中选中它，该会话即组合标准工具集与 `ncag_project`、`ncag_request`、`ncag_status`、`ncag_events`、`ncag_decide` 工具，沿用同样的 `NCAG_*` 配置，并与该平台的任何其他挂载按数据库共享同一个编排器。

<a id="guides"></a>
## 指南

| 指南 | 内容 |
|---|---|
| [部署](deployment.zh.md) | 配置、数据库、凭据与第一个项目 |
| [运维](operation.zh.md) | 控制平面路由、运维控制、预算与恢复流程 |
| [智能体档案](agent-profiles.zh.md) | 十六个内置专业化及新增方式 |
| [协议](protocols.zh.md) | 任务与智能体生命周期、消息类型、租约协议、决策记录、Git 集成 |
| [安全](security.zh.md) | 权限、工作区隔离、不可信内容、审计完整性 |
| [验证](verification.zh.md) | 如何判定完成、保存哪些证据、如何复现 |
| [测试](testing.zh.md) | 套件证明什么，以及如何运行 |
| [故障排查](troubleshooting.zh.md) | 运维实际会遇到的失败及各自含义 |
| [架构决策记录](decisions/README.zh.md) | 平台为何如此构建 |

<a id="what-ncag-will-not-do"></a>
## NC-AG 不会做什么

- **不会臆造模型路由。** 没有提供方、没有价格、没有模型时，任务报告缺失路由，而不是在想象的提供方上运行。
- **不会接受口头成功报告。** 当已记录的验证证据表明各标准通过时，任务才会完成。
- **不会假装工具运行过。** 每次工具执行都连同结果被记录；不可用的 shell、git 或检查命令会使调用失败并指出所查找的可执行文件。
- **不会隐藏失败的检查。** 发现与证据连同严重度与状态始终可见。
- **不会静默服从检索内容中的指令。** 工作区内容被框定为数据，形似指令的标记会被报告。
