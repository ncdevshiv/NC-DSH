---
description: "NC-AG 包分组：自主多智能体编排平台，从持久化领域记录到控制平面。"
kind: "package-group"
---

# packages/ncag

[English](README.md) | 中文

## 摘要

NC-AG 接收复杂指令，分析意图，质疑矛盾之处，把工作分解为依赖图，在相互隔离的 Git 工作树中派生专职智能体，协调它们的通信与资源占用，审查并独立验证它们的产出，交付一份有证据链支撑的结果。该分组承载领域词汇与生命周期状态机、带哈希链审计日志的存储、智能体运行时、编排器、控制平面，以及把平台挂载到 harness 循环之旁的插件。

没有任何模块在缺少已执行证据的情况下报告成功，也没有任何集成在验证配置之前宣称某个提供方或工具可用。

## 目录

- [包](#packages)
- [各部分如何协作](#how-the-pieces-fit)
- [相关文档](#related-documentation)
- [开发者说明](#dev-note)

-----

<a id="packages"></a>
## 包

| 包 | 职责 | ctx 键 |
|---|---|---|
| [`ncag-core/`](ncag-core/README.zh.md) | 领域词汇：带品牌标识符、记录类型、智能体与任务生命周期状态机，以及十六个内置智能体档案 | 库 |
| [`ncag-store/`](ncag-store/README.zh.md) | 持久化平台存储：SQLite 模式与迁移、类型化仓储、平台事件流与哈希链审计日志 | 库 |
| [`ncag-runtime/`](ncag-runtime/README.zh.md) | 智能体执行：与提供方无关的模型端口、受限工作区工具、提示词装配与智能体轮次循环 | 库 |
| [`ncag-orchestrator/`](ncag-orchestrator/README.zh.md) | 编排：规划、DAG 调度、智能体生命周期、资源租约、Git 集成、审查、验证、决策、成本与恢复 | 库 |
| [`ncag-dashboard/`](ncag-dashboard/README.zh.md) | 控制平面：HTTP API、服务器事件流、操作面板与平台进程入口 | 库 |
| [`tool-ncag/`](tool-ncag/README.zh.md) | harness 集成：平台服务插件与面向模型的 `ncag_*` 工具 | `ctx.ncag` |
| [`ncag/`](ncag/README.zh.md) | 门面包，把平台各包作为单一导入重新导出 | — |

<a id="how-the-pieces-fit"></a>
## 各部分如何协作

一次运行只有一条路径。用户请求通过 harness 工具 `ncag_request`（或控制平面 API）进入，编排器对它进行规划：确定性分析器规范化需求并构建任务 DAG。调度器取出依赖已完成且尚有余量的任务，为所需能力解析模型路由，派生一个档案实例，并在写租约下给它一块隔离工作树。智能体运行时驱动模型调用其工具，并记录每一次调用。智能体报告完成后，编排器执行所需审查，随后验证，随后集成；只有证据通过，任务才能进入 `COMPLETED`。每次调用都会记录用量：token 计数始终记录，只有已定价路由才记录 USD。配置了 USD 上限的项目会在账本越线时暂停并请求决策。

`ncag-store` 是持久状态的唯一写入者，每个动作都追加到哈希链审计日志，因此修改一条记录会破坏其后所有记录的验证。

<a id="related-documentation"></a>
## 相关文档

- [NC-AG 子系统](../../docs/subsystems/ncag.zh.md) —— 记录类型、生命周期状态机、通信与租约契约，以及审计链。
- [NC-AG 指南](../../docs/ncag/README.zh.md) —— 部署、运维、安全与验证流程。
- [NC-AG 架构决策记录](../../docs/ncag/decisions/README.zh.md) —— 平台为何如此构建。
- [Bundle 集成](../../packages/bundle/ncag/README.zh.md) —— dsh bundle 与 `ncag` 档案。

<a id="dev-note"></a>
## 开发者说明

<details>
<summary>维护者工作背景 —— 点击展开</summary>

NC-AG 有意拥有自己的存储，而不是扩展 harness 会话日志：平台运行许多并非 harness Agent 的智能体，它们的持久性必须在 harness 重启后存活，且不得触碰会话格式。与 harness 的唯一集成点是模型端口，它经由 `ctx.llm` 路由，因此提供方配置、凭据与重试仍由 harness 负责。

</details>
