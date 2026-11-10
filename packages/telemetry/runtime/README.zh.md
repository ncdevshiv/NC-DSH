---
description: "进程级追踪与指标：模型调用、工具调用、轮次与插件失败的有界跨度/事件环形缓冲，并以实时 JSON/SSE 诊断端点提供。"
kind: "package-reference"
---

# @deepseek-ai/dsh-telemetry-runtime

[English](README.md) | 中文

## 摘要

拥有 `ctx.telemetry`，即进程级追踪面。它订阅 harness 自身的接缝——`llm/stream`、`tools/execute`、`session/event`——记录有界跨度、计数器、按会话与按路由的汇总、崩溃记录，以及 Loader 的 fiber 状态。不写任何日志文件，热路径上的唯一成本是向固定容量环形缓冲写入一个数组项。

这些环形缓冲通过 `/api/diagnostics` 以三种方式提供，任何本地智能体或运维都可读取：

| 路由 | 含义 |
|---|---|
| `GET /api/diagnostics?events=N` | 以 JSON 返回完整快照 |
| `GET /api/diagnostics/stream` | SSE：一帧 `snapshot`，随后推送每个新跨度 |
| `POST /api/diagnostics/client` | 浏览器跨度上报（`[{ kind, name, durationMs, ok?, detail? }]`） |

## 目录

- [使用本包](#use-this-package)
- [理解实现](#understand-the-implementation)
- [已知限制与后续工作](#known-limitations-and-deferred-work)

-----

<a id="use-this-package"></a>
## 使用本包

把本行挂到 Web 应用旁的宿主平面。智能体可在其 shell 中读取实时统计：

```sh
curl -s "$DSH_WEB_URL/api/diagnostics" | jq
```

快照包含进程事实（pid、运行时长、RSS、堆）、计数器（`model.calls`、`tool.calls`、`turn.completed`、`span.slow`、`client.rpc` 等）、每个跨度名的 p50/p95/最大值延迟、按会话的轮次/步数/工具调用/token 及其最后路由、按路由的调用与错误计数及首字延迟分位、Loader 行及其 fiber 状态、最新崩溃记录与堆栈，以及最新跨度。当某个插件 fiber 失败或最近一分钟内发生崩溃时，`health.ok` 为 false 并给出具名原因。

其他插件可通过 `ctx.telemetry.record`、`count`、`crash`、`client` 记录自己的跨度。

<a id="understand-the-implementation"></a>
## 理解实现

服务为每类流各持一个 `Ring`（跨度/事件、浏览器跨度、崩溃），容量来自 `Config`；另有一个计数器表、每个跨度名一个时长桶，以及每个会话和每个 provider/model 路由各一个桶。记录是同步的：一次入环、一次计数自增，以及向各订阅者分发（逐个隔离）。

插桩只做订阅，被追踪的包无需改动：`llm/stream` 瀑布被包裹以测量首字与整体耗时并读取 usage 片段；`tools/execute` 围绕其 `next()` 计时；`session/event` 汇总轮次、步数与工具计数，并从 `turn/start` 到 `turn/end` 测量轮次耗时。

HTTP 路由注册在宿主 webserver 上，只接受回环调用者（或 `Host` 为回环地址的请求），因为快照会暴露路径、模型与会话标识。

<a id="known-limitations-and-deferred-work"></a>
## 已知限制与后续工作

- **仅宿主侧跨度。** 浏览器跨度在客户端上报后才有；随发行版交付的 Gateway 客户端会上报每次 Remote 往返，而更细的 UI 计时（工具卡首帧、悬停到展开）仍需各界面自行插桩。
- **不持久化。** 环形缓冲仅存在于进程内，重启即重置；持久追踪需要存储单元与保留策略。
- **无告警。** `health.reasons` 即信号，目前不会通知运维。
