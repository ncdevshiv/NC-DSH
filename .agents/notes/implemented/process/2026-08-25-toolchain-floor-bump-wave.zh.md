# Agent Note: 覆盖 JS 与 Python 工具链的版本下限批量提升

Status: implemented

[English](2026-08-25-toolchain-floor-bump-wave.md) | 中文

## 问题

两个生态的开发工具下限均已漂移：精确固定的 `oxlint` 落后于当前类型感知规则的修复，`@types/node` 停留在 22.x 线上而 `package.json` 声明的 engines 为 `^22.19 || >=24`，Python 构建与测试下限（`hatchling`、`pytest`）落后于当前主版本，且所有以 caret 固定的开发工具解析到的锁文件条目新旧不一。贡献者与 CI 实际运行的工具版本因此偏离上游当前的 lint 与类型行为。

## 决策

一次 chore 变更将每个下限提升到其当前下限线版本，并通过 `bun update` 解析全部 caret 区间：

| Package | From | To |
| --- | --- | --- |
| oxlint | 1.76.0 | 1.79.0 |
| oxlint-tsgolint | 7.0.2001 | 7.0.2001 (unchanged; prebuilt native binary per platform) |
| @types/node (root) | ^22.20.0 | ^24.0.0 |
| @types/node (apps/web) | ^22.0.0 | ^24.0.0 |
| mermaid | 11.16.0 | 11.17.0 |
| knip | ^6.16.1 | resolved 6.32.2 |
| tsdown | ^0.22.2 | resolved 0.22.14 |
| publint | ^0.3.21 | resolved 0.3.24 |
| jscpd | ^5.0.12 | resolved 5.0.16 |
| fast-check | ^4.8.0 | resolved 4.9.0 |
| hatchling (sdk + sdk-runtime) | 1.30.1 | 1.32.0 |
| pytest (sdk) | >=8.0 | >=9.0 |

驱动因素：oxlint 1.79.0 带来类型感知规则集修复，其 oxlint-tsgolint 各平台预编译二进制保持兼容；`@types/node ^24.0.0` 与最低声明引擎匹配，不再落后两条线；hatchling 1.32.0 携带当前 wheel 构建默认值；pytest 9 是当前主版本。`pydantic` 有意保持在 `>=2.12,<3`，因为 SDK 刻意使用 2.x API。该变更只触及依赖下限——不修改任何源码文件。

同一浪潮产生的同级决策分别记录：Vitest 通配 peer 下的 jsdom 30（[testing](../testing/2026-08-25-jsdom-30-under-vitest-wildcard-peer.md)）、React 19 / Vite 8 Rolldown 整合（[process](2026-08-25-react19-vite8-rolldown-consolidation.md)）、ruff 与 mypy 采用（[process](2026-08-25-python-ruff-mypy-adoption.md)），以及因第三方阻碍而暂缓的 TypeScript 7 升级（[proposed](../../proposed/process/2026-08-25-typescript-7-third-party-blocker.md)）。

## 验证

在提升后的代码树上，`verify-composition-references` 通过（所有插件引用均可解析），pre-push lefthook 门禁集——translation pairing、lint、third-party notices、whitespace、vendor guard——在暂存集上通过。更严格的工具暴露出的三个信号在父分支 `dev` 上即已存在，归本记录之外所有：新 oxlint 规则集下 `apps/web/tests/` 中 40 个 `expect(...).toContainText(...)` 错误、关于 `./src/*` 导出的 `publint` 警告（仓库级既有模式），以及 constraints 门禁下 `apps/desktop/package.json` 的 `files:` 不匹配。

## 曾考虑的替代方案

**按住每个下限，直到某个特性需要它。** 不予采用：类型落后于 engine 区间与过时的 lint 规则是每次运行都在支付的隐性成本，而非提升变更本身的成本；pre-release 立场偏好修正基础而不是携带漂移。

**把每个包拆成独立的提升变更。** 不予采用：依赖下限提升与源码无关，按包评审只增加开销而不隔离风险，批量处理使锁文件解析在一次变更中保持一致。

## 后果

贡献者与 CI 运行当前的 lint、类型、构建与测试工具，锁文件解析固定在提升后的下限上。更严格的 oxlint 规则集使三个既有基线缺陷持续可见于门禁输出；这些缺陷仍归本记录之外的所有方负责。未来的下限提升从本表出发，而不是从提升前的漂移出发。
