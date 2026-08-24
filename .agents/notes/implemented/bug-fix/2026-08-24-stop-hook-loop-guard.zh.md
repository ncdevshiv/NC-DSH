# Agent Note：Stop hook 循环防护

Status: implemented

[English](2026-08-24-stop-hook-loop-guard.md) | 中文

## 问题

两个 hook 桥接都把阻塞的 Stop 结果映射到 `agent/turn-stopping` 上的 `agent.steer()`，并且每个 Stop payload 都报告 `stop_hook_active: false`。因此，无条件阻塞的 hook 会在每个停止边界重新武装 continuation：每次阻塞都会强制一个完整的模型请求，而没有任何机制约束次数。唯一预期的防线是 hook 作者利用桥接从未置真的标志进行自我限制，所以一个行为异常的 `hooks.json` 意味着每轮次无上限的开销。

## 决策

每个桥接都维护一个 `StopBlockLedger`（位于 `dsh-hook-protocol`），以活跃 agent 实例和开放轮次号为弱键。在运行 hook 之前读取账本是 `stop_hook_active` 如实报告的关键：轮次首个边界为 `false`，该轮次已经历过一次强制 continuation 之后为 `true`。当合并结果为 deny 且额度已用尽时，桥接记录一条点名 `maxConsecutiveStopBlocks` 的警告（正整数校验的配置项，协议默认值 `DEFAULT_MAX_CONSECUTIVE_STOP_BLOCKS` = 25）并让边界提交；额度未满时照旧记录一次阻塞并 steer。

计数覆盖自轮次首次尝试停止以来的整条 continuation 链，而非严格不间断的阻塞。验证固定方式：协议包的单元 spec 在所有平台运行并固定账本语义；每个桥接用真实循环启动一个始终阻塞的 hook，断言请求次数被封顶、payload 标志序列为 `[false, true, …]`，以及跨轮次额度刷新。

## 已考虑的替代方案

- **严格连续计数** —— 否决：block/allow 交替的 hook 的不间断计数永远不超过一，但每个周期仍会强制一次请求。
- **用 `session/event` 的 `turn/end` listener 重置每会话计数器** —— 否决：为了派生轮次号本已携带的事实而增加订阅与清理；轮次作用域的账本键无需任何 listener 即可重置额度，且 WeakMap 键随 agent 实例消亡。
- **仅依赖 `stop_hook_active`**（参考实现的模型）—— 此处否决：未修改的第三方 hook 会忽略该标志；只有硬上限才能在它们不自我限制时约束开销。
- **把 `{"continue": false}` 作为运行级停止来执行** —— 属于另一项控制（`TODO(hook-continue-false)`）；本次变更刻意只覆盖 deny→steer 路径。

## 后果

一个阻塞 Stop hook 至多将轮次延长 `maxConsecutiveStopBlocks` 次 continuation；后续阻塞会记录警告并关闭轮次，每个新轮次都以全新额度开始。自我限制的 hook 行为与之前完全一致，且现在拥有与参考 wire 语义一致的如实 `stop_hook_active`。SubagentStop payload 仍报告 `false` —— 该点依旧只观测。
