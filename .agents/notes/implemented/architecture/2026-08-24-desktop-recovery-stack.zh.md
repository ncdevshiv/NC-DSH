# Agent Note: The desktop dev loop got a three-layer recovery stack

Status: implemented

[English](2026-08-24-desktop-recovery-stack.md) | 中文

## 问题

桌面开发长期被一类故障主导：渲染进程侧的故障让窗口永久死掉，而每条恢复路径都需要人工介入。四种具体表现各自独立定位过根因：

1. **有损热替换** —— 编辑客户端插件会让 slot 条目在替换中途崩溃（teardown→rematerialize 窗口期内服务不存在），且崩溃的条目再也回不来：outlet 在同一个 `SlotErrorBoundary` 实例下渲染新注册，其 `failed` 状态保持为 true——活内容被一层永久崩溃面遮住。
2. **白屏** —— slot 层之上的替换失败（shell 关键条目应用时抛出未捕获的 `cannot create effect on inactive context`）会杀死已挂载的树；该层之下的一切自愈手段都变得不可达，也不存在后续 rebuild 来触发恢复。
3. **黑屏** —— Electron 主进程中一次导航失败就意味着一个永远是 `backgroundColor` 颜色的窗口：`did-fail-load` 只打日志、不做任何事。
4. **僵尸栈** —— 窗口已死的启动器仍占着 node 树和启动锁；两个并发栈随后在同一份 `apps/web/dist` 上竞争 `vite build --watch`（Windows 的 EBUSY 会直接杀死 watcher），过期产物链还会产生误导性的渲染协议错误。

## 决策

恢复能力现在存在于三层，各自由真正能执行它的进程负责：

**渲染进程（`packages/client/hmr`、`packages/client/ui-renderer`）** —— `SlotErrorBoundary` 订阅复活通道；每次替换落定后 client-hmr 在 window 上派发 `dsh:hmr-swapped`，崩溃过的边界重置自己的崩溃面，让子组件带着重新执行的 inject 工厂按当前服务重新挂载。client-hmr 还会在 teardown 前静默（double-rAF + macrotask），以退避（0/500/2000ms）重试失败的替换，并把 cordis 的 inactive-context 拒绝视为重试预算的终态——绝不能在半应用的 generation 之上再次应用。

**Electron 主进程（`apps/desktop/main.mjs`）** —— 监督刻意放在唯一一个坏掉的渲染进程杀不死的地方：`did-fail-load` 以有界指数退避重试导航（仅主框架，突发首条才记日志），`render-process-gone` 做有界重载，空白像素 watchdog 每 10s 采样一次 `capturePage()`——连续三帧纯色（纯色即死亡；真实渲染的 chrome 总会有变化）就强制重载，预算为每 10 分钟 3 次。最小化/隐藏的窗口会被跳过：它们的截图是空的，与死掉的渲染进程无法区分。

**启动器（`scripts/dev-desktop.mjs`）** —— lockfile 单实例守卫拒绝第二个栈（`--replace` 杀掉旧树并接管；`--force` 由调用者自担风险并行启动），`scripts/dev-web.ts` 对每个 watch 阶段做有界重启监督而不是在第一次 EBUSY 就死掉。`apps/web/vite.config.ts` 在 watch 模式跳过 public 目录拷贝，作为第三层 EBUSY 防线。

**编写期门禁** —— 悬空插件类（引用不存在包的行能通过自洽性检查、只在宿主启动时爆炸）现在由 `bun run verify-composition-references` 把关并接入 hygiene：组合 YAML 中每个 `name: '@deepseek-ai/…'` 都必须解析到工作区包、已安装包或声明的子路径导出。

## 测试

在 Windows 上实机验证：`--replace` 接管、干净启动到已渲染帧（经 PrintWindow 像素采集）、组合门禁对注入悬空引用的通过与失败、完整客户端 typecheck。PR 前欠账：boundary-reset 路径的单元测试，以及全天改动仍需拆分为干净提交。

## 已考虑的替代方案

**只靠渲染进程 watchdog 自动刷新页面。** 否决，理由是不充分：bootstrap 空窗期里，替换驱动或 JS context 已死的渲染进程无法运行自己的修复。只有主进程监督覆盖这一类，这正是像素 watchdog 放在主进程的原因。

**用每实例独立 dist 目录替代单实例守卫。** 否决：为一个没有已知用例的工作流（两个并发开发栈）付出双倍磁盘与构建时间；`--replace` 已覆盖真实场景。

**改用 Vite dev-server HMR 替代 build + stat-poll 链。** 暂时否决：宿主按设计服务构建产物（插件 bundle 经客户端模块系统到达）；把 shell 切到 dev server 是比加固现有机制大得多的改动。
