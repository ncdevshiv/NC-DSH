# Agent Note：整分节提供方只能重置，永远不是删除

Status: implemented

[English](2026-08-23-whole-section-provider-reset-framing.md) | 中文

## 问题

Models 页面对每一行已配置的提供方都提供**删除**操作，但对整分节提供方——即 `deepseek-official`，其可配置提供方条目携带空的 settings path——任何写入都无法移除该行。它的 profile 就是 namespace 本身：即使用户层为空，namespace 也会通过 schema 默认值解析；store 在 namespace 存在时就把这样的行标记为已配置；而 `dsh-llm-deepseek` 无条件注册其适配器路由与目录条目（[web 配置面](../architecture/2026-07-30-web-config-plane.md)）。删除确实正确地清空了用户分节与页面管理的密钥——写入从来不是缺陷。

缺陷在措辞框架。原始 profile 渲染的是恢复默认值的描述，但任何一个用户层覆盖都会把 `removable` 翻成 true，于是对话框声称*「删除 {provider} 会移除其配置」*，而该行在操作成功后却被结构性保证仍然留在页面上。想要摆脱 DeepSeek 的用户删除后发现该行（或在没有任何其他可用提供方时的首启设置卡片）仍在页面上——这就是被上报的「删除却删不掉」缺陷。

## 决策

- `restoresBase` 现在覆盖所有整分节目标、不再看作者归属：`settingsPath.length === 0 || row.removable !== true`。路径寻址的目标仍由作者归属决定——用户编写的 pi-ai profile 删除后其行真正离开列表；与组合基线一致的则重置。
- 破坏性控件与其对话框按 `restoresBase` 在两套文案族之间选择：删除（`deleteTitle`、`deleteDescription*`、`deleteConfirm`、`deleting`）对重置（`resetTitle`、`resetRestoresBase*`、`resetConfirm`、`resetting`）。写入不变：同样的幂等 unset、同样的顺序，凭据先于 profile（[提供方凭据生命周期](../bug-fix/2026-08-06-provider-credential-lifecycle.md)）。
- 两套字典中已不可达的 `deleteRestoresBase*` 键被删除；两份 README 均写明整分节行无法被移除、其操作是重置。

## 已考虑的替代方案

**真删除：用户层墓碑门控适配器注册与目录条目，穿透 `ConfigurableProviderView`，并为受依赖方制定级联策略。** 暂缓否决。它会推翻已有决策（路由在没有用户层 profile 时仍保持注册；目录声明归组合所有），需要为「行可以消失」补上重新添加的 UX，并牵出未决的受依赖方——组合的默认模型选择与 DeepSeek 搜索提供方都指名 `deepseek-official`，会继续指向一条已被移除的路由。那是需要独立设计的产品功能，不是一次措辞修复。

**对整分节行隐藏该操作而不是改名。** 否决：重置覆盖与已存密钥是真实需求，藏起控件会把用户推向手工编辑 `settings.yaml`，同时让误导性说法无处修正。

**保留删除标签、只修正描述文本。** 否决：按钮与标题是第一眼读到的框架，自定义 profile 的场景仍会被读成删除。

## 后果

- 所有整分节内置提供方无论 profile 处于什么状态都渲染重置；pi-ai 的删除行为与文案原样未动。
- 来自环境的凭据仍在写入范围之外（既有且已记录）：对未识别目标的重置文案说密钥由其他位置管理并将保留，这一表述保持准确。
- 组件测试钉住两种整分节姿态——原始状态（不写凭据、根路径 unset）与带存储密钥的自定义状态（先 unset 凭据、再根路径 unset、进行中的「正在重置」标签）——连同未变的 pi-ai 删除钉子。
