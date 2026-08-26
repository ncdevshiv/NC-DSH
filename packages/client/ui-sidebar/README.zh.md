# @deepseek-ai/dsh-client-ui-sidebar

[English](README.md) | 中文

侧边栏外壳插件：负责品牌行、分区切换器、New Session 操作、布局持有的折叠控件、可感知滚动的分区区域 seat，以及固定在底部的 Settings seat。分区区域是 keyed slot `sidebar.section`——[ui-home-section](../ui-home-section/README.md) 渲染 Home，[ui-workspace](../ui-workspace/README.md) 渲染 Code 浏览器，Work 与 Team 键等待各自的分区包。本包只持有切换器外壳；既不派生任何分区的行，也不持有其视图偏好。折叠到布局拥有的 56px 轨道仍属于本地呈现行为。约定：[slot 系统标准](../../../.agents/notes/implemented/architecture/2026-07-22-slot-type-chain-implementation.md)。

展开的品牌行把 `sidebar.brand.mark` 与 `sidebar.brand.name` 渲染为两个独立的 single slot，收起轨道则渲染同一个 mark slot。没有占位者时，外壳使用鱼形标记，以及带有构建期 7 位 `DSH_CLIENT_COMMIT_HASH` 徽标的 `DSH Local Build` 标签。部署包可以单独替换任一值，而无须替换 New Session 控件或轨道几何；声明感知的 `slots.inject()` 让这种包无论先于还是后于侧边栏激活都能生效。

New Session 会启动运行时的页面局部前端 Session Intent。运行时优先使用作用域操作明确指定的 Workspace，否则使用当前 Session 所属 Workspace，再否则使用最近活跃 Workspace；一个 Workspace 都没有时则落入无工作区的聊天会话。外壳会在控件旁呈现该动作的结果：连接进行中显示忙碌标签，被拒绝的连接以内联告警呈现——高负载下一次创建可能耗时较久，在此反馈之前两者看起来都像按钮失效。Workspace 专属控件与共享选择器由 ui-workspace 持有。

分区切换器持有品牌行与页脚之间的区域。展开时它是 New Session 上方的药丸标签条（Home / Code / Work / Team）；收起时同样的四个分区成为共享轨道入口路径上的 36px 图标控件，点选其一还会展开侧栏（分区区域在收起时隐藏，只选区不展开将一无所见）。活动分区是外壳本地状态，启动时落在 Code；四个分区面板常驻挂在切换器之下（display 切换，与布局列同一模式），因此分区在多次访问与一次收起之间都保留其本地状态。

`SidebarRootComponentProps` 组合布局 owner share、全局 `useSessions` 和 `useWorkspaces` 钩子、已声明的品牌、keyed `sidebar.section` 与 `sidebar.settings` 子 slot，以及注入的 `startSession` 与侧边栏切换回调。这里没有插件 store。

分区切换只对进入的面板做动画：一次带方向的滑动 + 淡入（200ms），靠交替两个完全相同的 keyframe 名重新触发——同名动画重启是空操作——并由外壳按标签索引距离设置的 `--section-slide-from` 自定义属性定方向。减少动态效果模式会禁用该动画。

实时收起时，外壳会把展开内容固定在当前宽度，并用 150ms 将其淡出。随后，上方控件——外壳的侧栏切换、新建会话与分区轨道——共用一次 150ms 的淡入和 49px 左移，在布局的 300ms 栏滑动结束时一起进入 56px 轨道；每个 36px 控件盒都会沿同一条路径到达轨道左侧 10px 的内边距。固定在底部的 `sidebar.settings` 控件只共用淡入时序，不发生横向位移。页面初始即为收起状态时会静态渲染轨道；减少动态效果模式会禁用两段过渡。

栏内的滚动条是一种指针可供性：只要指针不在栏内，外壳就把 ui-theme 的[滚动条间接层](../ui-theme/README.md)重新绑定为 `transparent`；指针离开后滑块再保留 2 秒，因此没人指向的列表不会带着滚动条。避免行位移的空间预留属于滚动区域本身（[ui-workspace](../ui-workspace/README.md)），所以显示滑块不会引起重排。

页脚承载 `sidebar.settings`：侧边栏只渲染固定在底部的布局 slot，并共享其栏状态（`wide`）；ui-settings 在此注册触发行和设置面板。

`/client` 导出表层只包含插件主体（`apply`／`inject`）及约定类型；SidebarRoot、行组件和树派生仍由 slot 注册封装在包内。

## 模型体验

无。侧边栏渲染浏览器会话列表；这里没有任何内容进入模型请求。

#### KV Cache 影响

无；该包（package）既不组装也不发送提供方请求。

## 已知限制与暂缓事项

- **Session 状态点渲染由 [ui-workspace](../ui-workspace/README.md) 持有**：没有可用的 done/error 通知数据源。
- **Workspace 浏览行为由组合持有**：分组、排序、搜索与行状态都属于 [ui-workspace](../ui-workspace/README.md)，不属于此外壳。
- **「New task completed」未读标记是本地查看状态**：完成时间 > 上次查看时间这一事实永远不会到达宿主。
