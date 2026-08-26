# Agent Note: Vitest 通配 peer 声明下的 jsdom 30

Status: implemented

[English](2026-08-25-jsdom-30-under-vitest-wildcard-peer.md) | 中文

## 问题

jsdom 此前精确固定在 29.1.1，而 Vitest 4.1.11 将 jsdom 声明为通配 peer，因此这个 DOM 模拟依赖落后测试运行器已接受的主版本一个大版本，`@types/jsdom` 更是滞后在 28。

## 决策

jsdom 从 29.1.1 精确固定改为 caret `^30.0.0`（解析为 30.0.1），`@types/jsdom` 从 28.0.3 提升到 30.0.0。由于 Vitest 4.1.11 将 jsdom 声明为通配 peer，30.x 无需任何其他包变更即被支持。Playwright 已通过更早的[工具链提升浪潮](../process/2026-08-25-toolchain-floor-bump-wave.md)中 `apps/web/package.json` 的 caret 下限解析到锁文件中的 1.62.1，此处无需再处理。本决策与 [Vitest 对 jsdom/WebStorage 安排的所有权](2026-07-30-vitest-jsdom-webstorage-ownership.md)相邻：那条记录回答由谁提供 DOM 环境；本条只移动其大版本。

## 验证

应用两个升级后在干净状态上完整执行 `vitest run`：**13,658 通过 / 33 失败 / 77 跳过**（共 13,768 个测试）。全部 33 个失败均为既有的 Windows 环境限制——无开发者模式下的 EPERM symlink、`CreateProcessAsUserW` ACL-runner 失败、一处 `@adobe/react-spectrum` CSS 导入、超时，以及 shiki vm 产物——与迁移后基线记录的 52 个稳定环境失败形态一致。此前以级联形式失败的测试文件单独运行均通过，因此没有任何失败属于 jsdom 30 行为变化回归。

## 曾考虑的替代方案

**停留在 jsdom 29 精确固定上。** 不予采用：面对通配 peer 的消费方，精确固定毫无收益——Vitest 接受任何主版本——固定只会默默累积大版本债务。

**只升 jsdom 而不动 `@types/jsdom`。** 不予采用：28 年代的类型描述的是旧 API；让类型落后于运行时会给测试代码招来错误的类型报错，却没有换来任何隔离收益。

## 后果

DOM 模拟运行在 jsdom 30 与配套的 30 年代类型上，仍处于同一 Vitest 管理的环境约定之下。Windows 环境失败集保持其基线形态，未来完整套件运行以 13,658/33/77 为对照基准，而不是把 33 个失败中的任何一个当作新回归。
