# Agent Note: Completing the pnpm-to-bun migration across CI, release lanes, and product surfaces

Status: implemented

[English](2026-08-23-completing-the-pnpm-to-bun-migration.md) | 中文

## 问题

仓库已把包管理器切换为 bun（`packageManager: bun@1.4.0`、`bun.lock`、根级 `overrides`、`patchedDependencies` 与 `trustedDependencies`），但只有本地工具链随之迁移。每一条安装或经包管理器派生子进程的自动化泳道仍在驱动 pnpm：ci.yml 的五个作业、其他十个 GitHub workflow、GitLab Python 运行时 wheel 泳道、Wine Windows 门禁脚本，以及 `scripts/` 的大部分。由于 `pnpm-lock.yaml` 已删除，这些泳道根本无法安装——没有锁文件时 `actions/setup-node cache: pnpm` 直接硬失败，`pnpm/action-setup` 无法解析指向 bun 的 `packageManager` 字段，`pnpm install --frozen-lockfile` 也无从冻结。另有两条代码路径经由 `process.env.npm_execpath` 重启子脚本，在 bun 下它指向独立的 `bun.exe`：用 node 启动该文件会因 PE 字节抛 SyntaxError，破坏 `bun run build` 和所有门禁聚合。结果是本地开发正常、`bun run build` 在已验证路径上失败、每条 CI 泳道与两条发布管线全部当场瘫痪的仓库。

## 决策

bun 是所有泳道驱动的唯一包管理器。GitHub workflow 用 `oven-sh/setup-bun@v2` 加显式 `actions/cache` 步骤安装（缓存键基于 `~/.bun/install/cache` 和 `hashFiles('bun.lock')`；带架构限定的键在 setup-node 内置缓存需要平台区分的场景下仍可用），然后运行 `bun install --frozen-lockfile`。GitLab 运行时 wheel 作业下载根 manifest `packageManager` 字段钉住的 bun 发布版并执行同样的冻结安装。必须重启调用方包管理器的脚本检测独立二进制入口（`.exe` 后缀，或 `bun/` 前缀的 `npm_config_user_agent`）并直接派生该二进制；node 系管理器保持“node 下运行 JavaScript 入口”的形式。bun 下的 exec 式调用直接瞄准 `node_modules/.bin` 中随工作区安装的启动器，因为 `bun exec` 只解析 PATH 条目。

三处设计改变形状而非逐标志翻译：

- `scripts/build-exe-for-python-sdk.ts` 把运行时闭包暂存为独立迷你工作区——闭包 manifest 加上每个传递工作区包复制到 `deps/` 下，以 `--production --ignore-scripts --linker=hoisted` 安装——取代 `pnpm deploy --legacy --prod`；既有的链接物化步骤把工作区符号链接变成真实文件。
- `dsh plugin` 改转发 bun 而非 pnpm（`why` 映射到 `bun pm why`），profile 初始化改为写 `bunfig.toml` 钉住 `linker = "hoisted"` 而非 `pnpm-workspace.yaml`，git 依赖失败指引改指 profile `package.json` 的 `trustedDependencies` 而非 `allowBuilds`。
- `scripts/wine-windows-gates.sh` 以 `--os=win32 --cpu=x64 --linker=hoisted` 安装临时快照，复现了 pnpm 的 `supportedArchitectures` 加 hoisted-nodeLinker 覆盖的效果；bun 自身的旗标同时承担两者，上游的重命名竞态重试循环没有对应物可移植。

`verify-vendored-links` 把 `bun.lock` 当 JSONC 解析（行注释与尾逗号）并断言 workspace 规格而非 `link:` 条目；`gen-third-party-notices` 从根 manifest 读取成员 glob 与补丁，并从包含 bun `.bun/node_modules` store 的已安装树解析许可证元数据。`rescope-vendor` 的后置条件钉住根 manifest 的 `overrides` 条目，其 vendor README 行文描述该解析机制。

## 已考虑的替代方案

仅为 `pnpm deploy` 与 Landlock 打包拆分保留 pnpm 被否决：没有任何泳道还能引导起 pnpm——corepack 拒绝指向 bun 的 `packageManager` 字段，且所需锁文件已删除。为 Vitest 与 tsx 调用路由 `bun x` 被否决：bunx 在 Bun 运行时下执行 JavaScript bin，而这里每条泳道都在 Node 下运行它们；忠实的等价物是既有包脚本（`bun run test:e2e <files>`）和仓库标准的 `node --import tsx/esm` 启动器。让 Wine 泳道继续用 pnpm 因同样的引导问题被否决，而 bun 的 `--os`/`--cpu` 安装覆盖加显式 hoisted 链接用两个旗标复现了所需的 win32-x64 二进制物化。

## 后果

每条安装路径现在依赖同一工具链、同一锁文件、同一信任模型：`trustedDependencies` 在所有地方把关生命周期脚本，manylinux 的 node-pty 重建只挂载 node-gyp 头文件缓存，不再挂载已废弃的 setup 目录。bun 默认的隔离 linker 是一项活约束——任何需要扁平布局之处（Wine 快照、暂存的运行时闭包）都显式传 `--linker=hoisted`，且当 hoisted 树未点名某个包时，`verify-vendored-links` 与公告生成器会读取 `.bun/node_modules` store。只有真实打包安装才能证明的发布泳道行为——Landlock 入口 tarball 的 `bun pm pack` 文件名与可执行位处理、`bunx @yao-pkg/pkg` 在 Bun 运行时下驱动 SEA 打包——由发布 workflow 与打包安装演练覆盖，而非单元覆盖。`scripts/build.ts` 与门禁运行器的修复通过在 bun 下模拟其精确 spawn 形态完成验证；完整端到端 `bun run build` 还需要有提交记录的仓库，因为客户端构建记录绑定 HEAD 哈希。
