# Agent Note: Web notification center

Status: implemented

[English](2026-08-26-web-notification-center.md) | 中文

## 问题

宿主已经掌握了两个 Web 用户看不到事实:`dsh-notifications` 通知注册表(宿主插件向其中发布系统通知)和 `dsh-sidecar-updates` 升级管线(其为 AI SDK 发布暂存版本)。两者都没有任何浏览器界面,所以插件发布的通知在进入 transcript(文本记录)之前不可见,而暂存的更新在下次引擎启动前一直静默。

放置位置没有明显答案。框架声明恰好四个子 slot——`sidebar`、`conversation`、`details`、`shell.overlay`([ui-layout](../../../../packages/client/ui-layout/README.md))——其中没有应用级顶栏。shell 中唯一的会话无关图标簇是侧栏底部的累加式 `sidebar.footer.action` 列表,位于设置席位旁,Cordis inventory 面板已经以累加方式占据它。

## 决策

一个新的插件包 `dsh-client-ui-notifications`,贡献一个 `sidebar.footer.action` 条目(`notifications-bell`):一个铃铛触发器,其下拉面板容纳通知列表和 AI SDK 更新卡片。

- **协议契约先于网关冻结。** 网关的 `updates`/`notifications` 域并行落地,所以包在本地声明其窄的结构面(`UpdatesFace`/`NotificationsFace`,基于 `{ result: ok | err }` 信封),并从连接句柄读取。当 `IApiClient` 声明这些域时,本地类型被逐字替换为 `Pick<IApiClient, 'updates' | 'notifications'>`。
- **每个域拥有自己的内联错误行**(`sdkError`、`noticesError`)。成功只清除自己的行;一次成功的通知拉取不得抹除更新域的错误,反之亦然。被拒绝的写入跳过其尾随重读,因为写入没有改变服务端事实,而一次新的读取会清除它刚浮现的错误。
- **写入先于渲染落定**:read/dismiss/install/skip 在落定后重取,从不乐观更新。状态新鲜度依赖条目挂载(立即同步、60 秒轮询、聚焦重取);通知在打开和变更后拉取。
- **注意力分裂到两个指示器**:徽标计数未读且未关闭的通知;独立的圆点仅在没有未读通知竞争时才标记一个已提供且未忽略的更新。
- **安装成功文案从落定的重读推导**,而非从请求:`installedNow` 显示"下次启动生效"及发布说明链接,并在后续状态报告另一个可用版本时退役。

验证:包源码的逐文件 100% 覆盖率;组件 spec 通过冻结的 fixture(`tests/wire-fake.client.ts`)驱动 store,断言徽标/圆点计数、打开/关闭、安装 busy→成功文案切换、skip/check 流程,以及两条错误行。

## 后果

Web 用户现在不读 transcript(文本记录)或日志就能看到系统通知和暂存的 SDK 更新,并能从一个面板采取行动(读、关闭、安装、跳过),其中每个值都由宿主提供。侧栏底部成为应用全局控件的家:在 Cordis 面板旁已存在第二个累加式占据者,所以未来的全局界面应优先此席位而非新的 shell slot。本地声明的协议面是刻意的债务,有机械的退役路径;在此之前,这两个域的网关形状漂移由集成而非本包编译器捕获。

## 替代方案

| 被否决 | 一行理由 |
|---|---|
| 会话头部 utilities slot | 会话作用域:没有活动会话时铃铛消失,而这些事实是应用全局的 |
| 新的框架级顶栏 slot | 为一个占据者发明 shell slot;页脚 action 列表是现成的累加式席位 |
| 现在就从宿主包导入视图类型 | 使客户端耦合到并行后端尚未落地的产物;本地结构类型保持本包可编译,且可逐字替换 |
| 一条共享错误行 | 跨域清除使失败在不相关的成功上闪烁消失 |
