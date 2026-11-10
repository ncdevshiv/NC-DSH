---
description: "dsh Web 客户端上的 NC-AG 设置页：实时监控宿主平面平台，并编辑其智能体使用的模型、价格与上限覆盖。"
kind: "package-reference"
---

# @deepseek-ai/dsh-client-ui-settings-ncag

[English](README.md) | 中文

## 摘要

在设置中新增一个 **NC-AG** 分区，紧跟在 **Agent 预设** 之后。该页面监控自主多智能体平台——健康状态、用量、生效路由、项目、任务、智能体、决策与事件时间线——并编辑其智能体使用的模型覆盖、价格与上限。监控读取 `ncag` Remote 命名空间；配置表单读取 `ncag-settings` 设置命名空间。

## 目录

- [使用本包](#use-this-package)
- [理解实现](#understand-the-implementation)
- [延伸阅读](#further-exploration)
- [模型体验](#model-experience)
- [已知限制与后续工作](#known-limitations-and-deferred-work)

-----

<a id="use-this-package"></a>
## 使用本包

打开 **设置**，在 **Agent 预设** 下方选择 **NC-AG**。

**概览** 展示审计链状态、快照时间、模型调用与 token、花费、项目与活跃智能体计数、各层级解析出的模型路由及其生效价格、已注册的智能体档案，以及所有待决事项。

**项目** 列出工作流及其任务、智能体与待决事项数量；创建项目时若未给出仓库路径，则以第一个工作区为锚点。选中项目后可查看其任务（取消、按能力改派）、智能体（下达指示、取消）与决策。在此提交的工作请求会由平台归一化并规划。

**时间线** 列出最新的平台事件，并可打开某个关联标识的追踪——审计记录、事件、消息与工具执行。

**配置** 保存快速、标准与推理层级的覆盖（`provider/model`）、可选的 USD 价格表（JSON），以及活跃智能体、助手、步数、输出 token 与花费上限。0 表示关闭智能体、助手与 USD 上限；步数与输出 token 的 0 表示跟随部署默认值。

<a id="understand-the-implementation"></a>
## 理解实现

<details>
<summary>实现内部 — 点击展开</summary>

宿主半侧是空的 `apply`，仅用于让包持有 Loader 行，供客户端模块系统向浏览器半侧提供模块。`NcagMonitorController` 轮询 `ctx.remote.ncag.snapshot` 与 `recentEvents` 并提供操作动作；`NcagCardController` 通过绑定 `ctx.configForms.get('ncag-settings')` 的单个 `SettingsFormModel` 暂存每个配置字段。该分区以 `order: 25` 注册进设置外壳的 `settings.section` slot，因此紧跟在 Agent 预设之后渲染，其标题是随当前语言变化的 thunk，无需重新注册。未挂载平台行的部署仍会显示该页面，并把平台报告为不可用。

</details>

<a id="further-exploration"></a>
## 延伸阅读

- [ncag-controller](../../api/ncag-controller/README.zh.md) — 本页读取的 Remote 命名空间。
- [ncag-settings](../../ncag/ncag-settings/README.zh.md) — 配置页编辑的宿主命名空间。
- [ui-agent-preset](../ui-agent-preset/README.zh.md) — 紧邻上方的分区。

<a id="model-experience"></a>
## 模型体验

间接：保存的覆盖决定每个 NC-AG 智能体使用的提供方与模型，上限决定一个项目可消耗的智能体数、步数、token 与金额。

#### KV 缓存影响

模型覆盖只改变后续 NC-AG 智能体的路由，不会重写已有对话。

<a id="known-limitations-and-deferred-work"></a>
## 已知限制与后续工作

- **轮询。** 监控按间隔重新读取；推送式时间线需要 Remote 上的流方法。
- **步数与 token 的 0 跟随部署默认值**，因为智能体运行时要求有限数值。
