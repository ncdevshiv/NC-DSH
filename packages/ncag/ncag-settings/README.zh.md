---
description: "NC-AG 的宿主设置命名空间：按层级的模型覆盖、价格覆盖与可选上限，由 Web 设置页编辑。"
kind: "package-plugin"
---

# @deepseek-ai/dsh-ncag-settings

English | [中文](README.zh.md)

## 摘要

拥有 `ncag-settings` 设置命名空间。Web 设置页编辑它，NC-AG 平台在每次调度时读取它，因此保存的更改会在下一个项目或操作生效，无需重启。每个覆盖项都是可选的：模型字段为空时跟随用户主模型，价格表为空时只记录 token，上限为 0 表示不限。

## 目录

- [使用本包](#use-this-package)
- [理解实现](#understand-the-implementation)
- [已知限制与后续工作](#known-limitations-and-deferred-work)

-----

<a id="use-this-package"></a>
## 使用本包

把该行挂载在 Web 应用旁、`ncag` preset 的平台行之前。平台服务从全局服务存储解析 `ctx.ncagSettings`，因此 preset 不需要自己的设置副本。

| 字段 | 含义 |
|---|---|
| `fastModel`、`standardModel`、`reasoningModel` | 某一层级的 `provider/model` 覆盖；为空时跟随用户主模型 |
| `pricingOverrides` | JSON 价格条目；为空时只记录 token |
| `uiMaxConcurrentAgents` | 每项目活跃智能体上限；0 表示不限 |
| `uiMaxAgentsPerTask` | 每任务助手上限；0 表示不限 |
| `uiMaxStepsPerTask` | 每任务模型步数；0 跟随部署默认值 |
| `uiMaxOutputTokens` | 每次调用输出 token；0 跟随部署默认值 |
| `uiMaxCostUsd` | 项目花费上限；0 表示不限 |

<a id="understand-the-implementation"></a>
## 理解实现

`runtimeSettingsOf()` 把存储值转换为 `NcagRuntimeSettings`：解析每个 `provider/model` 文本、解析价格覆盖，并把 0 上限映射为 `null`（不限）。平台采样该视图，因此设置文档是唯一事实来源，无需重启。

<a id="known-limitations-and-deferred-work"></a>
## 已知限制与后续工作

- **不在此渲染用量分析。** 设置页只编辑配置；每项目用量由 `ncag_status` 工具和控制平面报告。
- **步数与 token 的 0 跟随部署默认值。** 智能体运行时要求有限数值，因此这两项上限无法真正无限。
