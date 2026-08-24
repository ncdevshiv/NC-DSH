# Agent Note: core-deps auto-sync from upstream branches

Status: implemented

[English](2026-08-24-core-deps-auto-sync.md) | 中文

## 问题

NIO 预设的依赖底座位于 `core-deps/`，是 `github.com/ncdevshiv/dataworm`、`openmem` 和 `ai-sdk` 的纯目录副本。它们都不含 `.git` 元数据，没有任何记录标明副本对应的提交，树中唯一的更新器（dataworm 的 `dw up`）重装的是*全局*工具而非这些副本。因此向上游分支推送在本地毫无效果：sidecar 持续执行过期代码，也没有任何信号提示存在更新的功能或修复。

## 决策

`core-deps/sync.mjs` 负责更新生命周期；`core-deps/run-synced.mjs` 是启动包装器，`agent.cordis.yml` 中两个 MCP sidecar 行都经由它启动。每次 NIO 运行、解释器启动之前，包装器会先应用任何被推迟的换装，执行带 TTL 缓存的 `git ls-remote` 漂移检查（默认 6 小时，`DW_COREDEPS_TTL_HOURS`），并在默认策略 `DW_COREDEPS_MODE=auto` 下浅克隆锁定分支并应用，使本次运行直接执行上游 HEAD。`notify` 只记录日志后继续；`off` 完全跳过网络；`DW_COREDEPS_OFFLINE=1` 强制静默离线。

安全性质由 `sync.mjs selftest` 在真实本地 git 远端上验证：运行时本地状态（`.venv/`、openmem 根目录的 `config.json`、`data/`、`.dataworm/`、`.nc-code/`、编译产物 `.pyd`/`.so`、SQLite/图数据库、缓存）永不被覆盖或删除；文件删除仅限 `core-deps/sync-state.json` 中上一次更新的清单范围；被覆盖的源码先落入 `.backup/<project>/<timestamp>/`；导入冒烟门（`dataworm.cli`、`mcp_server`）失败即回滚。被运行进程锁定的文件通过 `.pending/` 标记推迟到下一次启动——在任何 sidecar 存在之前应用。钉扎、清单与检查缓存持久化于 `sync-state.json`；`bun run coredeps:{check,update,status}` 手动驱动同一引擎。

更新以显式建议取代静默过期：`rust/` 变更意味着编译扩展需要重建（或退回 `--no-rust`）；`pyproject.toml` 变更意味着可能需要刷新依赖；ai-sdk 源码变更要求重建独立的 `nio-gateway` 构建。

## 已考虑的替代方案

- **在 `core-deps/` 下使用正式 git 克隆。** 否决：只有保持普通目录加旁挂状态文件，部署形态才得以保留——cordis.yml 中的绝对 sidecar 路径、项目目录内的机器本地数据、以及本仓库不跟踪这些树的事实。
- **在每个 Python sidecar 内部自更新**（如 `dw up`）。否决：按语言各复制一套引擎，且进程内更新器本就无法影响已在运行的 sidecar；在派生前应用（包装器）是更新对本次运行生效的唯一时机。
- **只检查、手动应用。** 被所有者需求否决——每次运行必须使用最新版本；它仍可作为 `DW_COREDEPS_MODE=notify` 使用。
- **cron 或 CI 定时同步。** 否决：漂移会落在与运行无关的时间点，且运行时检查仍需存在才能保证新鲜度。

## 后果

每次全新启动现在执行的都是锁定分支上的内容，上游质量门从"有人记得复制时"前移到"会话开始之前"；损坏的上游 HEAD 会败给自己的冒烟门并回滚，而不是拖垮代理。代价面：首次更新为每个项目支付一次完整克隆（dataworm 内嵌浏览器引擎树最重），Rust 变更后原生快速路径与网关二进制仍需手动重建，可复现性如今依赖 `sync-state.json` 记录实际运行内容——删掉它会无声地重新建立基线。信任这些仓库即是信任其所有者：供应链恰好就是 `github.com/ncdevshiv/*`。
