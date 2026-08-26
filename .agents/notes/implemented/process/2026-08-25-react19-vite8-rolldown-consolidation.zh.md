# Agent Note: React 19 与 Vite 8 将打包整合到 Rolldown

Status: implemented

[English](2026-08-25-react19-vite8-rolldown-consolidation.md) | 中文

## 问题

工作区背负两项大版本债务——Vite 6 与 React 18——并排运行两套打包引擎：`apps/web` 使用 Vite 6 的 JavaScript 引擎，而 `tsdown` 打包的一切使用 Rolldown（Rust 核心）。每次 web 应用构建都在支付第二套引擎的维护成本与行为差异。

## 决策

一次机械变更提升全部 43 个受影响包的 React 与 Vite 下限，并把 web 应用整合到 `tsdown` 已在使用的同一 Rust 核心打包引擎上，使整个仓库由单一引擎打包：

| Dependency | Scope | From | To |
| --- | --- | --- | --- |
| react | all 43 packages | ^18.2.0 | ^19.2.0 |
| @types/react | all 43 packages | ~18.3.1 | ^19.2.0 |
| react-dom | where pinned | ^18.2.0 | ^19.2.0 |
| @types/react-dom | where pinned | ~18.3.0 | ^19.2.0 |
| vite | apps/web only | ^6.0.0 | ^8.2.0 |
| @vitejs/plugin-react | apps/web only | ^4.0.0 | ^6.1.0 |

`apps/desktop` 不直接使用 React，保持不动。React 19 随行而来：`@vitejs/plugin-react` 6.x 与更新后的 `@types/react` 要求与其配对，且 react-dom 19 带来并发特性；一次变更高两个主版本符合 pre-release 的 "foundation over blast radius" 立场。diff 保持机械性：43 个 `package.json` 加 `bun.lock`，不触及任何 `src/`、测试或配置文件。一个一次性脚本应用了正则编辑，并在提交前移除。

## 验证

- `typecheck:contracts-ready`：exit 0，零 TypeScript 错误——React 19 更严格的类型约定无需对既有源码做任何 `ref` 转发修复、`useTransition` 形状调整或 `defaultProps` 弃用处理。
- 对两个最大的 JSX 重度包 `packages/client/runtime` 与 `packages/client/ui-conversation` 运行聚焦 vitest：53 个文件，824/824 测试通过。
- `apps/web` 生产构建 5.9 秒成功，输出结构不变（vendor + langs + index 切分）；>500 kB chunk 大小警告在本变更之前即存在。

## 曾考虑的替代方案

**只升 Vite，保留 React 18。** 不予采用：`@vitejs/plugin-react` 6.x 与 React 19 配对，只整合打包器会立刻重新引入这次单一变更本要消除的版本配对债务。

**web 应用继续用 Vite 6，不做任何整合。** 不予采用：这恰好保留本决策要删除的双引擎分裂，让 web 应用成为唯一脱离 Rust 核心而需要独立维护线的界面。

**先经过 Vite 7 再到 8 分两步走。** 不予采用：应用不消费任何 Vite 7 中间能力，多出的一跳增加一轮完整评审与重验证周期，却不会更早关闭任何一项大版本债务。

## 后果

整个仓库由单一 Rust 核心引擎打包，两项大版本债务在项目仍处 pre-release 时关闭。web 构建从此继承 Rolldown 的行为与诊断信息，React 19 的并发特性对客户端包可用，无需进一步的下限工作。
