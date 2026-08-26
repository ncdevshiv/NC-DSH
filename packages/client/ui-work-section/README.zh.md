# @deepseek-ai/dsh-client-ui-work-section

[English](README.md) | 中文

侧边栏 Work 区块：智能体实时工作板，渲染到 `sidebar.section` keyed slot 的 `work` 键。**Needs you** 置顶——被用户交互阻塞（审批、提问）的会话，人是瓶颈的地方。**Running** 列出所有执行中的会话，带状态点与最近活跃时间。**Goals** 渲染每个已物化 `goal` 投影值的会话，带阶段徽章（Active / Paused / Blocked）与阻塞原因；点击行即打开该会话。

所有数据都是对标准 `useSessions` hook 的纯派生——needs-you 与 running 是列表投影事实，对整个账户完整；goals 因物化范围而稀疏，待跨会话枚举落地（见下）。没有第二个订阅，也没有插件 store。

`/client` 只导出插件体（`apply`/`inject`）与契约类型；区块组件保留在包内部，藏在 slot 注册之后。

## 模型体验

无：Work 区块只渲染工作板，没有任何内容进入模型请求。

#### KV Cache 效果

无；本包不组装也不发送 provider 请求。

## 已知限制与暂缓事项

- **目标受物化范围限制** —— 会话目标只有在该会话于本浏览器打开后才出现（投影 store 由历史尾页填充）。完整的跨会话板需要按 api-proxy 模式新增一个小的宿主枚举 RPC（见整合报告）；UI 形状已保证该 RPC 无需改动组件即可接入。
- **后台任务与工作流运行尚未上板** —— 它们的镜像（`jobsBySession`、workflow-run 日志折叠）同样只覆盖已打开会话，上板会是同样的稀疏画面；随同一宿主枚举一并加入。
- **只读板** —— 线路已有 pause/resume（`goal.*`），但本区块把变更动作留给会话自身的 GoalBar 面。
