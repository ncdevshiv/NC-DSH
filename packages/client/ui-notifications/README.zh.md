# @deepseek-ai/dsh-client-ui-notifications

[English](README.md) | 中文

Web 通知中心 owner：向 `sidebar.footer.action` 贡献一个铃铛入口，点开承载系统通知列表与 AI SDK 更新卡片的下拉面板。两个板块都通过连接句柄读取冻结的 `notifications` 与 `updates` wire 面，因此本包不发任何会话级 RPC，除弹层开合外不持有任何状态，Host 始终是唯一事实来源。

触发器角标统计既未读也未忽略的通知数；当没有未读通知争夺注意力时，另一个圆点标记"有可安装且未被跳过的 AI SDK 更新"。打开面板会重新拉取通知列表，每次变更（已读、忽略）都在落定后重新拉取——绝不乐观渲染。点击行将通知标记为已读；✕ 控件忽略该通知。被忽略的行永不渲染；列表为空时显示空态文案。

更新卡片展示已安装与最新版本号。当存在未被跳过的新版本时，卡片提供 安装（执行中显示忙碌文案）、跳过此版本（写入忽略列表后重读状态）和 检查更新（显式刷新）。安装成功后卡片切换为"已安装 {tag}，下次启动生效"，并附上最新发布页作为发布说明链接；一旦后续状态报告又有新版本可用，该成功文案即退场。每个域有各自的行内错误行——本地操作失败加上状态视图的 `lastError`——任何失败都不会抛给组件。

状态新鲜度随入口的挂载生命周期：挂载即同步一次，每 60 秒轮询，窗口重新聚焦时再拉取。通知数据仅在打开面板与变更之后拉取。样式只用 token；文案走本包自己的 `notifications` locale 命名空间。wire 契约对齐并行落地的 API 网关形状而冻结，因此在 `IApiClient` 声明这两个域之前，本包在本地声明其最窄结构面。

## Model Experience

无：本包向人类呈现 Host 计算的通知/更新状态，不触碰 prompt、消息、schema、流或工具结果。读写都经由 wire 域；模型看不到该面板及其操作。

#### KV Cache effect

无；本包从不组装或发送 provider 请求。

## Known Limitations and Deferred Work

- **wire 面为本地声明** —— `UpdatesFace`/`NotificationsFace` 按冻结契约做结构镜像。待网关在 `IApiClient` 上落地 `updates`/`notifications` 后，本地类型将替换为 `Pick<IApiClient, 'updates' | 'notifications'>`；在此之前网关形状漂移无法被本包编译器捕获。
- **通知渲染是通用形态** —— 所有在世通知都以标题/时间/摘要渲染，不区分 `kind`，`data` 载荷被忽略。更丰富的分 kind 卡片应随第一个需要它的生产方一起到来。
