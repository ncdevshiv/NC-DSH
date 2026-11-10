---
description: "NC-AG 控制平面：HTTP API、服务器事件流、操作面板与平台进程入口。"
kind: "package-library"
---

# @deepseek-ai/dsh-ncag-dashboard

[English](README.md) | 中文

## 摘要

控制平面。`startDashboard(orchestrator, config)` 在一个 HTTP 服务器上提供面板文档、事件流与 API；`runPlatform(environment, llm, registry)` 为独立部署同时启动编排器与控制平面。每个路由都调用执行该操作的编排器方法，面板只渲染 API 返回的数据。

## 目录

- [路由](#routes)
- [面板](#the-dashboard)
- [事件流](#the-event-stream)
- [认证](#authentication)
- [已知限制与后续工作](#known-limitations-and-deferred-work)
- [开发者说明](#dev-note)

-----

<a id="routes"></a>
## 路由

| 方法与路径 | 操作 |
|---|---|
| `GET /` | 面板文档 |
| `GET /events` | 服务器发送的平台事件 |
| `GET /api/health` | 审计链状态、项目数与活跃智能体数 |
| `GET /api/snapshot` | 含任务与智能体的项目、用量、事件、路由、档案数 |
| `GET/POST /api/projects` | 列出 / 创建 |
| `POST /api/projects/:id/requests` | 提交请求；返回计划与任务 |
| `GET /api/projects/:id/{tasks,agents,messages,decisions,leases,waiters,changesets,events}` | 项目视图，含排队等待被占路径的智能体 |
| `GET /api/projects/:id/{profiles,requests,requirements,reviews}` | 已注册档案、项目请求、其规范化需求与审查运行 |
| `GET /api/tasks/:id/{changesets,evidence,findings}` | 单个任务的变更集、验证证据与审查发现 |
| `POST /api/projects/:id/{pause,resume}` | 项目控制 |
| `POST /api/tasks/:id/cancel` | 取消并释放 |
| `POST /api/tasks/:id/reassign` | 以另一种能力重新排队该任务 |
| `POST /api/agents/:id/cancel` | 取消一个活跃智能体并把其任务返回队列 |
| `POST /api/decisions/:id/resolve` | 记录结果或覆盖 |
| `POST /api/agents/:id/instruct` | 运维指令 |

请求体为 JSON，上限 1 MB。不是有效 JSON 的请求体返回带解析错误的 `500`；抛出异常的操作返回带编排器原因的 `500`。服务器绝不把失败的调用报告为成功。

<a id="the-dashboard"></a>
## 面板

一个无需客户端构建的文档：它获取 `/api/snapshot`，渲染项目、需求、任务、活跃智能体监视器、租约及其等待队列、审查运行、用量、时间线与审计状态，并通过 `EventSource` 订阅事件以刷新。控件包括创建项目、提交请求、暂停与恢复、取消任务、取消智能体、重新分配任务、向智能体发送指令、检视任务的变更集/证据/发现、过滤消息日志、读取决策历史，以及回答决策（有辩论记录时显示完整辩论）。每个控件都调用上文记录的 API 路由，控制平面规范会断言每个控件的路由都由服务器提供。

<a id="the-event-stream"></a>
## 事件流

`/events` 写出 `data: <事件 JSON>` 帧。服务器订阅一次 `store.subscribe` 并转发给每个已连接响应，在 `request close` 时移除该流。客户端断开不会停止平台；面板会自动重连。

<a id="authentication"></a>
## 认证

设置 `token` 时，每个 API 调用都需要该 bearer 令牌。事件流接受以 `?token=` 查询参数传入的令牌，因为 `EventSource` 无法设置首部。无令牌时 API 开放，因此 `host` 默认 `127.0.0.1`。

<a id="known-limitations-and-deferred-work"></a>
## 已知限制与后续工作

- **无按用户授权。** 一个令牌掌管整个控制平面。多操作者部署需要用户身份，属于 harness 远程访问层而非本包。
- **面板是单一文档。** 它有意保持免构建；面向档案与预算的更丰富编辑器应属于 harness 设置界面，待使用方需要时再实现。

<a id="dev-note"></a>
## 开发者说明

<details>
<summary>维护者工作背景 —— 点击展开</summary>

`ui.ts` 以字符串常量导出手册文档，使本包不附带浏览器构建产物。路由处理器位于 `routes.ts` 并接收 `RouteContext`；新增路由意味着在那里加一个分支、在面板加一个控件，二者都必须调用同一个编排器方法。

</details>
