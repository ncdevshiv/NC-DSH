# @deepseek-ai/dsh-client-ui-team-section

[English](README.md) | 中文

侧边栏 Team 区块：与你协作的智能体，渲染到 `sidebar.section` keyed slot 的 `team` 键——**活跃成员优先**。成员即会话列表中的 subagent 行（当前存在的智能体，运行中或空闲，最新优先、运行中置顶）；点击成员经运行时的 subagent 地址路由打开其会话。下方是花名册：部署的 agent preset 渲染为可启动的队友——名称、信任徽章（System/User）、描述，损坏状态诚实呈现而非留下失效按钮。「Start session」创建空白会话并挂载该 preset（`agentPresets.select` 拒绝已启动的会话，因此采用动作落在 connect 返回的新空白会话上）。

花名册数据经 `agentPresets.list` 线路面写入一个快照 store，并在 `connection/reset` 时刷新（preset 编写落盘为文件，线上没有其他通告）。成员数据是对标准 `useSessions` hook 的纯派生——除花名册快照外没有第二个订阅，也没有插件 store。

`/client` 只导出插件体（`apply`/`inject`）与契约类型；区块组件保留在包内部，藏在 slot 注册之后。

## 模型体验

无：Team 区块只渲染花名册与成员列表，没有任何内容进入模型请求。

#### KV Cache 效果

无；本包不组装也不发送 provider 请求。

## 已知限制与暂缓事项

- **成员活动仅是列表投影** —— 每成员的转录下钻与审计视图（OpenBot 的 Activity 标签概念）暂缓；成员行打开既有会话面。
- **尚无团队分组** —— Buzz 的 `AgentTeam` 概念（带共享指令的 preset 命名分组）暂缓；花名册渲染平铺 preset 列表。
- **花名册仅在重连时刷新** —— 本浏览器保持连接期间新编写的 preset 要到下次刷新或重连才出现；编写时的刷新由 settings 面为其自身视图负责。
