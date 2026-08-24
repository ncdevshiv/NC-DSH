# Agent Note: landlock-run 由 C11 重写为单文件 Rust

Status: implemented

[English](2026-08-23-landlock-run-rust-port.md) | 中文

## 问题

启动器曾是仓库中唯一一个内存安全完全依赖评审纪律的组件：它是不可信命令行最先抵达的进程，负责设置 `no_new_privs` 以中和沙箱内的 setuid/setgid 提权，而它的失败模式是静默的策略失效而非崩溃。C 在这里不提供任何兜底——授权解析中的越界错误会直接作为限制缺陷发布出去。工具链选择同样限制了原生平台布局：Windows 进程树终止的缺口（尽力而为的 `taskkill /T /F`）只有依靠内核保证的 Job Object 语义才能闭合，而第二套需要人工审计的 C 代码库会让这一环更难保持可信。

## 决策

启动器现在是 `native/landlock-run/packages/entry/native/` 下的单文件 Rust crate（`Cargo.toml`、`src/main.rs`、`Cargo.lock`）。它唯一的依赖是 `libc` crate 的 syscall shim；Landlock UAPI 结构体与访问位仍在源码文件内逐字自定义，保住「定义即审计记录」规则。release profile 固定 `panic = "abort"`，因此未来的任何 panic 仍然失败闭合——进程在 exec 被包装命令之前即告终止。

CLI 约定（[cli-contract](../../../../native/landlock-run/docs/cli-contract.md)）逐字节不变：argv 文法、所有启动器失败的退出码 `125`、探测报告行以及部分强制执行通知。`test/launcher.test.js` 针对真实二进制验证这些字符串且原样通过。入口包照旧在 tarball 中附带经审计的源码，现在位于 `native/` 下并连同 `Cargo.lock`。

构建仍然按架构只做本机编译。`scripts/build.ts` 使用 cargo 配合 Rust 自带的静态 musl target：`rustup target add <triple>` 只安装标准库，rustc 为 musl target 自带自包含 CRT 目标文件，宿主 C 编译器仅承担链接驱动——不再需要 `musl-gcc`。二进制仍落在 `packages/<name>/bin/landlock-run`，因此 `verify-launcher-binary.mjs` 与打包演练无需修改即可通过。CI 的正式构建作业在 apt 步骤旁补上 rustup target。

## 测试

- `test/launcher.test.js`——用法错误、退出码透传、拒绝写落盘的世界证明、跨 exec 继承、授权根不可打开时失败闭合——在 Linux CI 腿上于 `NALR_REQUIRE_LANDLOCK=1` 下原样通过。
- 入口测试、ELF prepack 门禁与打包安装演练针对相同的产物和约定；`cargo check` 与 `cargo clippy` 在两个 musl triple 上零告警通过。

## 已考虑的替代方案

**保留 C11 源码。** 驳回：代码已经过审计且约定稳定，但每一次加固或平台扩展都会重新打开人工内存安全评审，而就安全论证而言这门语言没有提供任何编译器强制保证之外的贡献。

**采用社区的 `landlock` crates.io crate。** 驳回：它会把 UAPI 定义从被审计的文件移入传递依赖版本，用「自定义即审计记录」换取零功能收益——启动器只需要三个 syscall 和两个结构体。

**`no_std` Rust。** 驳回：argv 处理、分配与 `execvp` 本就需要 std；`panic = "abort"` 加上单 crate 且锁定的依赖集合已经圈定了运行时面，去掉 std 只会增添 unsafe 管道。

**改用 Zig 重写。** 驳回：同样的安全动机配上更年轻的工具链；静态 musl 产物今天就是 Rust 的一等 target，启动器没有任何 Zig 特有能力的需求。

## 后果

- 限制边界处的内存安全不再单独依赖评审，审计面仍是一个经过评审的源文件加一个锁定依赖。
- 构建信任基新增 rustc、std 与 `libc`，由 `Cargo.lock` 和 CI 构建镜像固定；消费方依旧只收到预构建静态二进制，新增面停留在构建期而非分发期。
- 分配失败经由 Rust 分配器中止，不再打印 `landlock-run: out of memory`；两条路径都在 exec 前终止，而约定只固定致命前缀格式，该格式得以保留。
- 构建启动器需要 cargo 与 musl std target 而非 `musl-tools`；工作区命令文档已载明该前置条件。
- 未来的原生限制类工作（例如规划中的 AppContainer/Job-Object Windows runner）将继承一个安全关键核心由编译器强制不变式的语言编写的模板。
