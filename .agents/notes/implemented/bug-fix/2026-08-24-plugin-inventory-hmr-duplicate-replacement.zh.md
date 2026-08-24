# Agent Note: The plugin inventory replaces a running HMR duplicate instead of failing the toggle

Status: implemented

[English](2026-08-24-plugin-inventory-hmr-duplicate-replacement.md) | 中文

## Problem

在每个 `dsh web` 宿主上，通过 Web 设置界面启用 HMR 行总是失败：

```
Toggle failed: failed to apply loader entry hmr (@deepseek-ai/cordis-plugin-hmr): service "hmr" has been registered at <Hmr>
```

两个组合事实造成了不可避免的冲突。`dsh-web-app` 组合包禁用了共享的模块重载 `hmr` 行（`# TODO: Re-enable shared HMR for Web after its reload lifecycle is tested.`），而 `profile-boot` 随后通过 `loader.create` 挂载一个仅监视配置的 HMR 回退实例（`root: []`）——因为文档承诺的 patch 层热重载约定要求即使该行被禁用也必须存在 `hmr` 服务。于是被禁用行的设置卡片许诺了一次永远无法成功的启用：挂载它会在同一作用域内启动第二个实例，Cordis 以响亮失败拒绝重复的服务注册，Loader 把条目回滚为禁用，网关把整条原始错误链抛给了用户。

## Decision

两处修改让切换对实际运行的状态保持诚实：

- **网关替换而非失败**（`packages/host/plugin-inventory`）。在一次真实的启用失败之后，`setEnabled` 收集激活中的同模块条目——相同 `options.name`、fiber 正在运行、非 group 行——先停用它们，重试一次启用；若替换本身失败，则恢复每一个被替换的条目。替换只在真正碰撞之后触发，因此提供相互隔离实例的条目永远不会被打扰；没有重复实例时的无关失败会原样到达调用方。
- **patch 监视跟随 `hmr` 服务**（`@deepseek-ai/dsh-app-boot`）。启动时的注册从两次直接调用 `watchUserPatches` 改为经由 `watchUserPatchesAcrossHmrSwaps`，它把注册放进一个要求 `hmr` 的 `user-patch-watch` 插件中。Cordis 会让这些注册随其所属实例一起卸载，并对下一个挂载的实例重新执行，因此替换回退实例后 `cordis.patch.yml` 热重载无需重启即保持存活。首次应用仍在启动时收敛，并经既有的抑制路径响亮失败；后续应用通过各自的 fiber 状态暴露失败。

## Alternatives considered

**以指明运行中重复实例的消息拒绝启用。** 诚实但无用：切换依然不可用，除非用户手工完成交换，而手工顺序（先停用回退实例）会悄悄终止 patch 监视——正是第二处修改所消除的损失。更清晰的报错只是给死胡同化了妆。

**让 profile-boot 复用配置行而不是创建回退条目**（在无服务时以覆盖后的配置就地启用 `include:hmr`）。这劫持了一个被组合包禁用的行的含义，违背明确的 TODO 把完整的模块重载 HMR 带上 Web，并且用户再把该行关掉时依旧损坏。

**解析重复注册的错误文本来决定何时替换。** 与 vendor 诊断字符串的脆弱耦合；可观察的行为触发条件（启用失败且存在同模块运行实例）无需解析即可判定，并且能自我纠正——如果替换不是解药，重试会失败且一切被恢复。

## Consequences

在 Web 上启用被禁用的 HMR 行现在可以成功：完整模块重载实例在本次进程内替换仅监视配置的回退实例，被替换的条目保持停用直到下一次切换或重启（web-app TODO 对未测试重载生命周期的告诫，从此归属于拨动开关的人）。反向陷阱依设计仍然可见：停用最后一个 HMR 实例会让 patch 监视停止，直到某个实例再次挂载或进程重启；这一点没有被任何机制掩饰。

当启用因无关原因失败而同模块 sibling 恰好运行时，会出现一次短暂扰动：sibling 在注定失败的重试前后被替换又恢复。恢复通常成功，因为被替换的模块片刻前还在运行；若恢复失败，失败信息会逐一点名未被恢复的条目。

## Testing

`packages/host/plugin-inventory/tests/inventory.spec.ts` 固定替换成功、合成重试失败后的恢复、以及无关失败的原样返回。`packages/boot/app-boot/tests/user-patches.spec.ts` 在存活监视器下交换两个 loader 条目形式的 HMR 实例并要求编辑经由第二个实例生效，另覆盖缺少 include 的响亮失败路径。
