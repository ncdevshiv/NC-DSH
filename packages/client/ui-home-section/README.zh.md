# @deepseek-ai/dsh-client-ui-home-section

[English](README.md) | 中文

侧边栏 Home 区块：以收件箱形式渲染到 `sidebar.section` keyed slot 的 `home` 键。概览条（运行中会话、会话总数、工作区数）位于 New Session 快捷操作与最近会话收件箱之上——收件箱按最新优先列出非空会话，每行携带状态点（运行中 / 需要你 / 已完成）、agent preset 标签与紧凑相对时间；点击行即打开该会话。本区块通过 `ctx.slots.inject('sidebar.section', …)` 注册，因此与外壳的激活顺序无关，注册随调用方 fiber 一同卸载。

所有数据都来自标准全局 hook（`useSessions`、`useWorkspaces`）；仅有的动作是运行时共享的 `startSession`/`open` 动词，由 apply 闭包注入。没有插件 store：收件箱是对会话列表快照的纯派生，最多渲染 30 条最新非空行。

`/client` 只导出插件体（`apply`/`inject`）与契约类型；区块组件保留在包内部，藏在 slot 注册之后。

## Model Experience

无：Home 区块只渲染会话列表概览，没有任何内容进入模型请求。

#### KV Cache 效果

无；本包不组装也不发送 provider 请求。

## Known Limitations and Deferred Work

- **收件箱仅是列表投影** —— 尚无未读跟踪；「已完成」依赖运行时的离开期间完成位，会话打开即清除。
- **无跨会话目标/工作流聚合** —— 该面属于 Work 区块（ui-work-section）；Home 刻意保持为会话收件箱。
- **相对时间在渲染时计算** —— 空闲时标签不跳动；在下一次列表投影变更时刷新。
