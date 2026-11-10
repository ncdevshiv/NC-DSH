# Agent Note：把 NC-AG 暴露为 Web Agent preset，并按数据库共享一个编排器

Status: implemented

[English](2026-11-07-ncag-web-preset-and-shared-platform.md) | 中文

## Problem

NC-AG 平台此前只能通过独立的 `ncag` 档案或显式 `--patch` 加载 `packages/bundle/ncag/cordis.patch.yml` 才能使用。随发行版交付的 Web Agent 选择器只列出 `standard`、`ptc`、`minimal` 与 `cordis`，因此 Web 用户无法从随发行版交付的界面启动该平台。

按会话分别挂载平台还暴露了一个调度缺陷：每个挂载都在同一个 SQLite 数据库上构造自己的编排器，因此两个会话——或 preset 与独立 bundle 同时挂载——都可能看到同一个 `READY` 任务并各派发一次。此外，控制平面文档化的 `/trace` 路由调用了编排器上不存在的方法，导致宿主类型检查失败，并使该操作一直没有实现。

## Decision

### 增加 `ncag` preset，而不是再写一套平台组合

`packages/bundle/web-app/presets/ncag.patch.yml` 声明第五个随发行版交付的 preset（`order: 5`）。它复用标准工具集，并增加一个 `cordis:group`，以 `isolate: { ncag: true }` 承载平台服务与 `tools-ncag`：

- preset 拥有它所挂载的服务，因此平台必须位于隔离域；否则挂载会以 `Preset services require isolate realms: ncag` 失败，而且该服务会活得比会话更久。
- preset 读取文档化的 `NCAG_DATABASE_PATH`、`NCAG_WORKSPACE_ROOT`、`NCAG_MODEL_PRICING` 与调优变量，两个路径默认位于 `$DSH_HOME/ncag`。
- `NCAG_MODEL_PRICING` 默认 `'[]'`。价格是可选覆盖；token 记账始终记录，设置页中的价格条目才启用 USD 记账。[跟随主模型的决策](2026-11-08-ncag-settings-owned-models-and-limits.zh.md)取代了此前“空表拒绝挂载”的“不猜价格”要求。

### 按解析后的数据库路径共享一个编排器

`@deepseek-ai/dsh-ncag-service` 维护模块级的“数据库路径 → 引用计数的 `SharedPlatform`”映射。首个挂载构造存储与编排器；后续挂载等待同一个 `started` promise 并增加计数。释放最后一个挂载会停止编排器并关闭存储。`:memory:` 路径按挂载分别建键，因此彼此私有。

每个数据库一个平台正是杜绝重复派发的前提：编排器的派发循环是 `IN_PROGRESS` 转换的唯一写入者，第二个实例会与之竞争。

### 记录一次运行的关联标识并实现 trace

`audit()` 接受本次运行的关联标识，任务运行路径把已有的 `correlation` 传入 spawn、工具调用、交接、安全、评审与集成记录；运行之外的动作用自己的标识。`NcagOrchestrator.trace(projectId, correlationId)` 读取该标识下的审计记录、平台事件、消息与工具调用，并解析它们引用的任务和智能体。控制平面的 `/trace` 路由调用它，因此文档化的操作与真实实现一致。

## Alternatives considered

- **把平台挂载到 Web 宿主平面。** 否决：每个 Web 会话都会启动平台并占用数据库，即使用户没有选择该模式；该服务也会活得比选择它的会话更久。
- **让每个挂载在同一数据库上各开一个编排器。** 否决：两个派发循环可能都认领同一个 `READY` 任务，且第二个实例的重启恢复会改写另一个实例的实时状态。
- **只通过独立档案提供 NC-AG。** 否决：Agent 选择器正是用户决定会话工作方式的地方；把这个 Web bundle 已经交付的模式藏在需要重启应用之后，等于让承载它的界面看不到它。

## Consequences

- 选择 **NC-AG 模式** 会启动一个随会话结束的平台；取消选择会停止最后一个挂载。
- 独立 bundle 与某个会话同时运行时，不会把同一个任务派发两次。
- 同一个标识现在把一次运行的审计、事件、消息与工具调用归为一组，也就是控制平面展示的 trace；运行之外写入的记录各自带标识，因而只形成单条 trace。
