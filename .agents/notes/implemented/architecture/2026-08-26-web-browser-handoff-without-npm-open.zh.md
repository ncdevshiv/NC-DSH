# Agent Note：`dsh web` 浏览器交接不再依赖 npm `open`

Status: implemented

[English](2026-08-26-web-browser-handoff-without-npm-open.md) | 中文

## Problem

`packages/bundle/web-app` 过去通过运行在辅助 Node 子进程里的 npm `open@11` 打开默认浏览器。这个子进程的存在只是为了承载该依赖：每次 Windows 启动都要付出一次 PowerShell 启动（约 1.2 秒的交接耗时），拖着一个 68 个文件的安装树，而它的凭据隔离只能透过 mock 观察。无论怎样，Bun 原生 `Bun.open` 的工作都要求移除该依赖。

## Decision

`packages/bundle/web-app` 在进程内打开默认浏览器：运行时提供内置 `Bun.open` 时直接调用，否则以 `scrubbedParentEnv()` 直接 spawn 平台开启器（`cmd /c start`、`/usr/bin/open`、`xdg-open`）。npm `open@11` 依赖已删除，只为运行它而存在的辅助子进程架构也随之删除。

### 辅助子进程为何存在，以及为何删除它是安全的

旧设计 spawn 一个 Node 子进程来引入 `open`，理由有三：

1. **凭据隔离** —— 子进程拿到 `scrubbedParentEnv()`，因此 `DEEPSEEK_API_KEY` 永远不会抵达开启器。直接 spawn 在同一 seam 上保留完全相同的保证：`browser-opener.ts` 显式传入 `scrubbedParentEnv()`，且 `tests/browser-opener.spec.ts` 断言被 spawn 的环境中不存在凭据形态与 `DSH_*` 名称。在 Bun 下根本没有子进程，任何东西都不可能携带凭据。
2. **Windows 启动器等待** —— npm `open` 在 PowerShell *spawn* 时即解析，早于 URL 抵达 shell，因此需要一个比这段间隙活得更久的第二进程。直接 `cmd /c start` 路线在启动器自身退出时结算；不存在需要遮掩的间隙。
3. **Specifier seam 测试** —— 组装快照挂钩的是 `open` 模块说明符。替代的 seam 是 `internals.openBrowser`，fixture 直接替换它；快照的证据行保持不变。

### 迁移的两条契约

- **凭据不得出现的位置**从「开启器子进程的环境」（只有运行在被清洗子进程内的 mock 能观察到）变为「开启器的 spawn options」——由单元测试在真实的 builder/spawn seam 上精确断言。快照记录删除了 `apiKeyPresent`/`dshHomePresent`；这两个字段从来只能观察到 mock，而不是 OS 开启器。
- **快照生命周期** —— `dsh web` 没有自关停；旧的 happy-path 快照之所以记录了 `exitCode: 0`，只是因为录制运行恰好在超时窗口内被外部终止。测试 1 现在使用与其 SSH 同类相同的、有界的 `DSH_BROWSER_OPEN_TEST_EXIT_ON_READY=1` 机制，使组装级验证是确定的而非碰运气。

### 以 `exit` 而非 `close` 结算

开启器等待解析于启动器的 `exit` 事件。`close` 需要 stdio 管道排空，而 Windows 的 `start` 可能让这些管道归它派发的进程所有（控制台宿主的目标会继承它们），因此管道排空没有时间上限，而进程寿命有。直到退出为止累积的 stderr 仍是失败原因。

## Alternatives considered

**只等上游的 `Bun.open`。** 否决：dsh 今天仍跑在 Node 上，页面在那里同样需要启动器；为该情形保留 npm `open` 会原样保留下这次要删除的启动成本与依赖面。

**经由跨平台辅助脚本（例如打包一个脚本）间接调用，而非直接的平台命令。** 否决：那会重新制造一个职责只剩兼容性的子进程，而三个平台开启器各只需一条 argv。

## Consequences

- 交接端到端（调用 → 开启器结算）在 Windows、Node 24 上：**1236.9 ms → 79.1 ms 均值（15.6×）**；npm `open` 每次启动都要付出一次 PowerShell 启动。
- 删除了 `open` 依赖图每次交接 **32.8 ms** 的模块求值，以及安装树中 68 个文件 / 约 132 KB 的运行时依赖。
- 凭据隔离改为在真实 spawn seam 上断言，而非经由子进程内的 mock 观察；happy-path 快照的成功也不再取决于一次外部 kill 是否恰好落在超时窗口内。
