# Agent Note: 聊天模式与工作区切换

Status: implemented

[English](2026-08-23-chat-mode-and-workspace-shift.md) | 中文

## 问题

Web 客户端此前把一切对话都挡在 Workspace 选择之后：没有注册任何 Workspace 时，启动选择保持空闲，常驻输入框以"选择一个工作区开始"的占位符渲染为惰性态，New Session 也清空到同一个死胡同页面。用户在挑选或创建目录之前连一条消息都发不出去。Host 其实早已支持无 Workspace 的会话（`session.create` 不带 `workspaceId`/`cwd` 时默认取 Host cwd），但 UI 从未走过这条路。

另一方面，会话的 cwd 一经创建便不可更改（`SessionCwdConflict`；两种持久化后端都把日志存放在由 cwd 派生的目录下），因此无 Workspace 开始的对话永远无法被*移动*进某个 Workspace。对话中途选择工作区只能在两者之间取舍：丢弃当前对话（既有的空白会话复用路径），或让它继续无处归依。

## 决策

**聊天模式是一等状态，不是错误姿态。** 无 Workspace 的会话完全可用：输入框保持可编辑，其 chip 显示本地化的聊天标签（"聊天"/"Chat"）并配新建聊天图标，同时仍会打开 Workspace 选择器——选择器正是聊天转入工作模式的入口。只读的惰性 textarea 现在只覆盖真正无会话的状态（基线到达前的启动窗口）。

**聊天引导复用空白会话复用机制。** `WorkspaceRuntime.connectChat()` 与 `connectWorkspace()` 成镜像：优先从列表镜像复用未入账、未归档的空白会话，否则在 Host 上创建一个不带 Workspace 的会话；并发调用通过同一张 in-flight 映射在合成键下合并。注册的 Workspace 为零时，`startSession()` 与一次性的启动选择都会落入聊天会话，而不是清空选择。聊天会话一旦非空，就出现在侧边栏既有的 Ungrouped 桶中；空白会话照旧隐藏。

**切换是重定向 fork，不是移动。** `session.fork` 新增可选 `workspaceId`：子会话采用该 Workspace 的路径作为自身 cwd 并加入该 Workspace，而不是跟随源会话（或 subagent 最近的归属祖先），种子历史原样保留，而后续每一轮都在目标内运行与归组。种子仍然引用源 cwd；保留指的是上下文，而非搬迁。未知 id 返回 `workspace-not-found`；发布后附加失败沿用既有的 `workspace-attach-failed` 部分成功契约。wire 请求只携带 `workspaceId`；客户端把目标路径作为仅供显示的 `cwd` 提示传给乐观插入的子会话行。空白会话保留廉价的复用或创建路径并搬运草稿；只有非空会话才走 fork。

## 曾考虑的替代方案

- **附加时改写会话头 cwd**：会话头不可变，JSONL 日志实际存放于 cwd 派生的项目目录；移动意味着日志迁移，却没有持久收益。
- **用合成的上下文消息播种新会话**：在没有真实轮次的情况下改变模型可见内容，且重复了 fork 机制本就精确携带的历史。
- **按匹配 Host cwd 自动附加**：注册表刻意要求索引成员资格加规范化 cwd 双重匹配；放宽会让 CLI 产生的会话被悄然归组。

## 后果

- 全新部署可以零配置当作纯聊天使用；之后再添加工作区时，可通过选择器收编进行中的对话。
- 删除工作区后的 hero 场景（此前被迫回到"选择工作区"）现在退化为聊天模式——会话依旧可用，严格优于一堵墙。
- 反复切换聊天会积累未分组的非空会话；侧边栏的 Ungrouped 桶和归档功能就是现成的应对手段。
