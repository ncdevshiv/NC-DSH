# Agent Note：NC-AG 跟随主模型，并在设置页拥有自己的覆盖项

Status: implemented

English | [中文](2026-11-08-ncag-settings-owned-models-and-limits.zh.md)

## 问题

Web 端的 `ncag` preset 在挂载前要求 `NCAG_MODEL_PRICING`。`createModelPrices('[]')` 抛出异常，preset 激活记录为 `broken`，而 `packages/client/ui-agent-preset` 中只显示健康 preset 的 chip 会隐藏 **NC-AG 模式**：没有价格表的部署只能看到四个模式而不是五个。同一张表还带来两个后果：

- 没有部署环境变量，平台就无法启动，用户无法从随包发布的界面选择该模式。
- 每个路由都必须先声明价格，任何 agent 才能运行，即使记账并非用户关心的问题。

## 决策

### 用户主模型就是默认路由

`ModelRouter.resolve()` 接受会话主模型作为 `defaultRoute`，并在每个能力层级优先使用它。NC-AG 设置页的按层级覆盖（`override`）优先于默认值；两者都没有时，路由发现仍回退到 harness 注册表。preset 与独立 `dsh-bundle-ncag` 补丁在挂载时读取 `agentDefaultModel.currentSelection()`，因此从不编辑 NC-AG 设置的用户，其每个 NC-AG agent 都使用与主 agent 相同的模型。

### 价格是可选覆盖，token 不是

`modelPricing` 变成可选覆盖表（`'[]'` 可加载）。`ModelRouter.price()` 对未定价路由返回 `null`，`UsageRecord.costUsd` 记为 `0`，而 `CostManager` 仍写入包含真实输入、输出与缓存 token 数的账本行。token 记账因此不依赖价格。`CostManager.budget()` 只在配置了上限时执行；默认自治策略不带 USD 上限。

### 上限与覆盖项位于同一个设置命名空间

`@deepseek-ai/dsh-ncag-service` 声明由设置文档持有的 volatile Config 字段：按层级的模型覆盖（`fastModel`、`standardModel`、`reasoningModel`，写作 `provider/model`）、`pricingOverrides`，以及五个上限（`uiMaxConcurrentAgents`、`uiMaxAgentsPerTask`、`uiMaxStepsPerTask`、`uiMaxOutputTokens`、`uiMaxCostUsd`）。agent、助手与 USD 上限的 0 表示不限；步数与输出 token 的 0 表示跟随部署默认值，因为 agent 运行时要求它们是有限数。`runtimeSettingsOf()` 在每次 orchestrator 采样时解析这些字段，因此保存的更改会在下一次调度或下一个项目生效，无需重启。

### UI 表面

浏览器设置页通过 `ctx.configForms.get('ncag')` 读取同一个 `ncag` 命名空间，沿用 `ui-settings-subagent` 的页面模式。

## 考虑过的替代方案

- **发布猜测的默认价格表。** 否决：陈旧或错误的价格会在看似权威的情况下误报花费；用户可以改为在设置页输入真实价格。
- **完全移除成本记账。** 否决：账本仍是运行消耗的唯一证据，而且无论是否定价，token 计数都必须保留。
- **保留必需的环境变量并写文档。** 否决：这会保留隐藏模式的缺陷，而该缺陷正是本决策要解决的。

## 后果

- **NC-AG 模式**在没有任何环境配置的情况下出现在 agent 选择器中。
- 未定价的运行记录 token，并把成本报告为未知；希望在 USD 上记账的用户可在设置页为路由定价。
- 没有 USD 上限的项目不会因预算而暂停；并发与助手上限只在用户设置时生效。
- 独立的 `ncag` profile 保留其文档化的环境契约；只有随包发布的 Web preset 与 bundle 放宽了必需价格。

## 验证

- `packages/ncag/ncag-orchestrator/tests/units.spec.ts` 覆盖未定价发现、每个层级的主模型默认值，以及按层级覆盖。
- `packages/ncag/ncag-service/tests/service.spec.ts` 覆盖零价格挂载与设置解析。
- `apps/cli/tests/web-agent-presets.e2e.ts` 挂载 `ncag` preset 并断言 `ncag_*` 工具与服务。
