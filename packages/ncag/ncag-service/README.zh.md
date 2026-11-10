---
description: "NC-AG 平台服务插件：一个编排器、其持久化存储，以及经由 harness LLM 服务的模型端口。"
kind: "package-plugin"
---

# @deepseek-ai/dsh-ncag-service

[English](README.md) | 中文

## 摘要

把 NC-AG 平台作为 `ctx.ncag` 挂载到 harness 进程。它依据校验过的插件配置创建一个编排器，在 `ctx.llm` 存在时给它提供模型端口，启动它，并随插件生命周期处置它。它以 `ctx.get` 严格读取 LLM 与工作区服务，因此缺少它们的组合仍能启动；此时任务会报告缺少的运行时能力，而不是让整个档案崩溃。凡解析到同一绝对数据库路径的挂载共享同一个编排器：第二次挂载只增加引用计数，而不会打开一个相互竞争的调度器，因此两个会话——或 preset 与独立 bundle 同时挂载——都不会把同一个 READY 任务执行两次。`:memory:` 数据库则为该挂载私有。

## 目录

- [配置](#configuration)
- [服务 API](#service-api)
- [已知限制与后续工作](#known-limitations-and-deferred-work)
- [开发者说明](#dev-note)

-----

<a id="configuration"></a>
## 配置

| 字段 | 必需 | 含义 |
|---|---|---|
| `databasePath` | 是 | 平台数据库绝对路径 |
| `workspaceRoot` | 是 | 智能体工作区绝对目录 |
| `modelPricing` | 否 | JSON 价格覆盖；为空时只记录 token，不计 USD |
| `defaultRoute` | 否 | 用户主模型；未设置时跟随 `agentDefaultModel` |
| `fastModel`、`standardModel`、`reasoningModel` | 否 | NC-AG 设置页提供的按层级 `provider/model` 覆盖 |
| `pricingOverrides` | 否 | 设置页的 JSON 价格覆盖 |
| `uiMaxConcurrentAgents` | 否 | 活跃智能体上限；0 表示不限 |
| `uiMaxAgentsPerTask` | 否 | 每任务助手上限；0 表示不限 |
| `uiMaxStepsPerTask` | 否 | 每任务模型步数；0 跟随 `maxStepsPerTask` |
| `uiMaxOutputTokens` | 否 | 每次调用输出上限；0 跟随 `maxOutputTokens` |
| `uiMaxCostUsd` | 否 | 项目 USD 花费上限；0 表示不限 |
| `preferredProviders` | 否 | 按优先级排列的提供方名称 |
| `leaseTtlMs` | 否 | 租约时长，默认 900000 |
| `maxConcurrentAgents` | 否 | 默认活跃智能体上限，默认 4 |
| `maxStepsPerTask` | 否 | 每任务模型步数，默认 24 |
| `maxOutputTokens` | 否 | 每次调用输出上限，默认 4096 |
| `healthIntervalMs` | 否 | 观察间隔，默认 15000 |
| `stalledAfterMs` | 否 | 进度超时，默认 120000 |
| `repeatedToolThreshold` | 否 | 触发循环事件前的相同调用次数，默认 4 |

格式错误的 `modelPricing` 或 `pricingOverrides` 会让加载失败并指出出错条目与字段。空价格表是合法的：token 计数始终记录，只有已定价的路由才产生 USD。

<a id="service-api"></a>
## 服务 API

| 成员 | 含义 |
|---|---|
| `orchestrator` | 运行中的 `NcagOrchestrator` |
| `createProjectFromWorkspace(name, description)` | 在首个已注册工作区创建项目 |

<a id="known-limitations-and-deferred-work"></a>
## 已知限制与后续工作

- **控制平面不在此启动。** 平台服务运行编排器；提供面板服务是 [`ncag-dashboard`](../ncag-dashboard/README.zh.md) 包的职责，因此无头组合不会打开端口。

<a id="dev-note"></a>
## 开发者说明

<details>
<summary>维护者工作背景 —— 点击展开</summary>

服务与其工具位于不同包，因为 Loader 不能在同一模块中混用服务默认导出与函数插件：一个文件会丢弃另一个的命名空间。

</details>
