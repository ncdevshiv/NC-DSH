# Agent Note: Capture provider response metadata in the pi-ai adapter

Status: implemented

[English](2026-08-23-pi-ai-response-metadata-capture.md) | 中文

## Problem

诊断一次 pi-ai 路径上的重试失败——即[分类 pi-ai 传输层截断](2026-07-22-pi-ai-transport-truncation-classification.md)与[跟随 pi-ai 的暂时性错误措辞表](2026-08-23-pi-ai-transient-wording-parity.md)两条 note 将其路由到可重试 code 的那个传输截断家族——只能停在 harness code 上：失败既不携带 HTTP 状态也不携带提供方请求 id，而直连 DeepSeek 适配器的失败会保留 `x-request-id` / `x-deepseek-request-id` 以便支持方关联。同一 seam 下两个适配器之间的这种不对称没有任何归属。pi-ai 错误事件本身两者都不暴露：终止事件只交付一条被扁平化的消息字符串。

## Decision

- 每次 `PiAiAdapter.stream()` 调用都会通过 pi-ai 的 `onResponse` 流选项捕获状态与请求 id；每个随附助手协议都在响应头到达时调用它——先于响应体消费——因此即使响应体随后中途死亡，这些事实也已存在。
- 捕获到的事实填充 error 与 aborted finish 分片上缺失的字段（`LlmFailure.status`、`LlmFailure.requestId`），以及空闲超时 `LlmError`。调用方中止保持裸值，成功 finish 原样通过。
- 请求 id 的查找镜像 DeepSeek 适配器的优先顺序（先 `x-request-id`，再 `x-deepseek-request-id`），header 名大小写不敏感；已映射的失败字段绝不被覆盖。
- 包 README 把"无法获取提供方 HTTP 状态"的限制替换为准确的残留：状态仅来自这次边界捕获，因此在任何响应到达之前抛出的失败只暴露 code 而没有状态。

## Alternatives considered

**从扁平化的错误文本中解析 id。** 否决：id 存在于响应头而非消息文本中；截断措辞本身没有可解析的内容。

**通过 fetch/dispatcher/client 钩子在 pi-ai 之下观察响应。** 否决，理由与截断分类 note 已记录的一致：pi-ai 不暴露任何此类钩子。`onResponse` 是其认可的观察点，且它在响应头时刻触发，足以赶在响应体之前。

**把元数据附加到每个流式分片上。** 否决：这会把单一边界事实复制到数百个持久化日志分片中，而没有消费者按分片读取它。

## Consequences

- pi-ai 路由上的 `llm/retry` 事件与轮次失败如今像直连 DeepSeek 路由一样携带请求 id，因此即使重试已经恢复了轮次，网关运营者仍能把上报的会话与服务端日志关联起来。
- 被丰富过的 finish 分片正是循环写入日志的那一份，因此转写、派生失败与重试事件展示完全相同的事实。
- 从不触发 `onResponse` 的协议会让失败保持裸值——这是可观察的缺席，而不是错误归因的值。
