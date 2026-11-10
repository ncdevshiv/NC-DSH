---
description: "NC-AG 工具插件：面向模型的 ncag_project、ncag_request、ncag_status、ncag_decide 与 ncag_events 工具。"
kind: "package-plugin"
---

# @deepseek-ai/dsh-tool-ncag

[English](README.md) | 中文

## 摘要

在 `ctx.tools` 上注册五个面向模型的 NC-AG 工具。平台服务由 [`ncag-service`](../ncag-service/README.zh.md) 挂载；本插件需要 `tools` 与 `ncag`，并在激活时注册。

## 目录

- [工具](#tools)
- [模型体验](#model-experience)
- [已知限制与后续工作](#known-limitations-and-deferred-work)
- [开发者说明](#dev-note)

-----

<a id="tools"></a>
## 工具

| 工具 | 操作 | 返回 |
|---|---|---|
| `ncag_project` | 创建项目（未给仓库路径时默认当前工作区） | 标识、名称、状态、仓库路径 |
| `ncag_request` | 向项目提交请求并执行 | 创建的任务及其状态与能力 |
| `ncag_status` | 读取平台快照，可限定单个项目 | 审计状态、成本、任务与智能体的文本报告 |
| `ncag_decide` | 回答待决决策 | 决策标识、状态与结果 |
| `ncag_events` | 读取单个项目的时间线 | 按顺序的事件类型、时间戳与摘要 |

每个工具都调用执行该操作的编排器方法。`ncag_status` 首先报告审计链状态，因为断裂的链会使报告中其他一切主张失效。

<a id="model-experience"></a>
## 模型体验

### 请求上下文与条件

#### 模型所见

五个工具模式及其描述。它们向从未使用过平台的模型介绍该平台：创建项目、提交工作、读取状态、回答决策、读取时间线。

#### Token 影响

固定：五个模式与简短描述，不随对话规模变化。

#### KV 缓存影响

工具声明前缀只追加：模式是静态的，因此它们本身绝不会使已缓存前缀失效。

<a id="known-limitations-and-deferred-work"></a>
## 已知限制与后续工作

- **五个工具，而非整个控制平面。** 暂停、恢复、取消与发送指令是 HTTP API 上的运维操作；面向模型的接口只暴露智能体在任务中可能需要的五个操作。

<a id="dev-note"></a>
## 开发者说明

<details>
<summary>维护者工作背景 —— 点击展开</summary>

工具在执行时读取 `ctx.get('ncag')` 而不是注入它，因此工具插件与服务插件可以任意顺序加载而无待决依赖：注册表一旦存在即激活工具，而在平台尚未挂载时调用会准确报告这一点。

</details>
