# Agent Note：侧边栏分区切换器与 Home/Code/Work/Team

Status: implemented

[English](2026-08-25-sidebar-section-switcher.md) | 中文


## Problem

Web GUI 侧边栏原本只有一个浏览面：工作区/会话浏览器填满外壳唯一的 `sidebar.workspaces` 洞。多智能体 harness 需要呈现的其他内容无处安放——每个智能体此刻在做什么（目标、运行中的会话、等待某人的交互），以及智能体本身的花名册（可启动的 agent preset、可查看的活跃 subagent 成员）。把它们叠进工作区浏览器会埋掉会话列表；做成独立全屏又会让侧边栏无法回答「我在哪」。

## Decision

侧边栏外壳（`ui-sidebar`）持有一个**分区切换器**：展开时是 New Session 上方的药丸标签条（Home / Code / Work / Team），收起时同样的四个分区成为轨道上的 36px 图标控件（轨道点选同时展开侧栏，因为分区区域在收起时隐藏）。分区区域是 keyed slot `sidebar.section`（`kind: 'keyed'`，字面键联合 `SidebarSectionKey = 'home' | 'code' | 'work' | 'team'`），由外壳的 `sidebar` entry 声明；外壳在渲染点派发活动键，不传任何 owner props——每个分区的业务数据与动作经各自的 inject 到达。

一个 client 插件包注册一个键：

- `ui-home-section` → `home`：收件箱式概览（汇总条、New Session 快捷操作、最新非空会话与状态点）。
- `ui-workspace` → `code`：既有的 `WorkspaceBrowser`，从已废弃的 `sidebar.workspaces` single slot 重指向。浏览器简化为恒宽：外壳在收起时隐藏分区区域，因此浏览器的轨道图标分支、`wide`/`expandSidebar` owner share 与 `searchOnExpand` 机制连同其测试一并移除。其目录流洞更名为 `sidebar.section.directoryFlow`（两个 directory-picker 包同步重指向）。
- `ui-work-section` → `work`：实时工作板——「Needs you」（被用户交互阻塞的会话）置顶，其后是 Running 与带阶段徽章的已物化 `goal` 投影。
- `ui-team-section` → `team`：活跃成员优先（会话列表的 `origin: 'subagent'` 行，运行中置顶，经运行时的 subagent 地址路由打开），其后是作为可启动队友的 agent-preset 花名册（「Start session」创建空白会话并经 `agentPresets.select` 挂载 preset——该调用拒绝已启动的会话，因此采用落在 connect 返回的新空白会话上）。

活动分区是外壳本地状态，启动时落在 `code`（历史表面）。四个分区面板常驻挂载、以 display 切换（与布局列同一模式），分区在多次访问与一次收起之间保留本地状态。切换只对进入的面板做动画：带方向的滑动 + 淡入（200ms），靠交替两个完全相同的 keyframe 名（`section-in-a`/`section-in-b`——同名动画重启是空操作）重新触发，由外壳按标签索引距离设置的 `--section-slide-from` 自定义属性定方向。减少动态效果模式禁用该动画。

概念出处：花名册 → 频道 → 观察活动的循环沿袭 CopilotKit/OpenBot 的 coworkers 与 channels；智能体作为一等成员与审计友好的框架沿袭 block/buzz。两者都只是概念捐赠者——一切骑在既有 DSH 服务上（会话列表投影、`agentPresets`/`subagent` RPC、会话日志），没有新的后端进程、数据库或协议。

## Alternatives considered

**在 `ui-workspace` 内围绕浏览器做标签页。** 落败：一个 UI 特性一个插件包（`packages/client/AGENTS.md` 目录制度），Home/Work/Team 与工作区浏览器不共享数据或 store，而第五个分区会让一个不拥有它的包继续膨胀。

**带分区选择器的 `chain` slot。** 落败：标签集是封闭且外壳持有的，不是自荐的；chain 按落选选举一个 entry，适合接管路由（composer），不适合用户切换的固定标签集。

**在分区 owner share 上保留 `wide`/`expandSidebar`。** 落败：不再有分区在轨道模式渲染（区域被隐藏），该 share 没有消费者；包边界拒绝投机性 API。

**先做宿主侧跨会话工作枚举 RPC 再做 UI。** 暂缓而非拒绝：needs-you 与 running 今天就从列表投影完整可得，而 goals/jobs/工作流镜像只覆盖已打开会话；工作板带着诚实的稀疏 goals 区块发布，README 记录了无需改动组件即可接入的宿主 RPC 后续。

## Consequences

侧边栏获得了稳定的顶层导航词汇；未来的每个表面（每成员审计视图、收件箱式 Home 流、跨会话工作枚举）都以一次注册或分区本地改动落地，而非再次改动外壳。外壳的轨道契约改变了形状：区域的轨道图标被分区轨道取代，`sidebar.workspaces` 不复存在（pre-release 立场——连同所有引用一起重命名，包括由 `bun run gen-client-catalog` 生成的扩展 slot catalog）。依赖覆盖 `use-sync-external-store: ^1.6.0` 随本次改动必要落地：被钉住的 1.2.0 声明 React ≤18 peer 而工作区运行 React 19，bun 因此装了嵌套 React 副本，所有挂载 renderer 绑定 hook 的 jsdom 套件在干净 HEAD 上失败（约 620 个测试）。

## Testing

每包组件规格覆盖各分区的派生与动作臂；`ui-sidebar` 规格覆盖切换器（标签派发、轨道点选 + 展开、冷启动静态收起）、keyed 声明与样式表契约（rail-in 目标、section-enter keyframes、减少动态效果）。侧边栏 DOM 快照已重录。合并前以 `DSH_SNAPSHOT=replay test:web` 覆盖装配输出的变化。