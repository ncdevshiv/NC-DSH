# Agent Note: New Session 连接结果可见化，并为 session.create 设定客户端时限

Status: implemented

[English](2026-08-24-new-session-connect-outcome-surfacing.md) | 中文

## 问题

New Session 的所有触发点此前都是「发射后不管」：`startSession` 返回 `void`，两个外壳适配器丢弃调用，连接失败被压缩成一条 `console.warn`。连接本身可能合法地耗时数分钟——Host 用同一个进程服务所有会话的回合、折叠与压缩，而 `session.create` 在线的两侧都没有任何时限——因此在高负载下一次点击看起来就像按钮失效；被拒绝的创建则什么表现都没有。由于 host 会在 preset mount 完成前先持久化会话 header（`api-proxy.ts` 在 setup 前解析 id），一次被放弃的点击仍会在磁盘上留下永久的、只有 header 的空白会话。恢复全靠运气：负载高峰过去后，排队中的创建落盘，列表里悄悄多出一行空白。相关机制记录：[Workspace UI product flow](../feature/2026-07-25-workspace-ui-product-flow.md)、[chat mode](../feature/2026-08-23-chat-mode-and-workspace-shift.md)、[blank-reuse membership](2026-08-05-workspace-blank-session-reuse-membership.md)。

## 决策

- `IWorkspaces.startSession` 返回 `Promise<SessionId>`：在导航与草稿重置完成后 resolve；失败时先记录日志再 reject，携带业务原因。想要旧「发射后不管」形态的程序化调用方显式加 `.catch(() => {})`（agent-preset 的 creator draft）。
- 每个触发面在控件旁渲染结果反馈：忙碌标签加重入护栏，被拒绝的连接以 `role="alert"` 行呈现在触发点旁。护栏在每个面内共享，因为 New Session 现在总是新建（`forceNew`），并发点击会造成重复空白；该路径不经过运行时的 in-flight 合并。
- `SessionManager.create` 在 `SESSION_CREATE_TIMEOUT_MS`（30 秒，与 `streamOpenTimeoutMs` 同一 UX 尺度）向调用方结算错误码 `session-create-timeout`。迟到的结算仍会把已发布的空白会话合并进列表，最终创建成功的会话不会继续不可见。
- `scripts/dev-desktop.mjs` 把每个子进程的 stdout/stderr 同步写入 `<DSH_HOME>/logs/<label>-<timestamp>.log`；此前 host 侧诊断的唯一副本随启动器控制台消失，这类失败事后无从追查。

## 已考虑的替代方案

**全局 toast 服务。** 拒绝：client 技术栈中不存在这样的通道，为两个界面发明一个超出需要；按触发点的 `role="alert"` 行与既有局部错误惯例（重命名对话框、设置分区）一致。

**把连接失败写进 `WorkspaceListState.error`。** 拒绝：该字段属于列表拉取状态轴；动作结果是请求局部状态，在下一次成功拉取后还会残留。

**host 侧创建时限。** 此处拒绝：慢但最终成功的创建在高负载下属正常现象，服务端中止会遗留半挂载的 agent。约束调用方、同时让迟到结算照常合并，使客户端耐心与 host 事实彼此独立。

## 后果

- 缓慢或失败的连接在用户按下的控件处可见：侧边栏两种宽度以及浏览区域均覆盖；pending 期间的重复点击不再产生重复空白会话。
- 超时的点击之后仍可能出现会话行（迟到合并）。这是有意为之：会话在 host 上确实存在，隐藏它只会把原 bug 反向重现。
- 30 秒超时是固定的 UX 常量而非配置项；部署若需要不同取值，应与连接层各超时一起转为运行时配置。
- 验证：`manager.client.spec.ts`（超时结果、迟到合并）、`workspaces-service.client.spec.ts`（拒绝传播、不 open）、`sidebar-root.client.spec.tsx`（护栏、忙碌标签、告警、恢复）、`workspace-browser.client.spec.tsx`（内联告警），以及三个改动包的完整套件。
