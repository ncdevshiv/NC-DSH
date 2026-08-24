# Agent Note: Track pi-ai's transient-error wording table in classifyPiAiError

Status: implemented

[English](2026-08-23-pi-ai-transient-wording-parity.md) | 中文

## Problem

[pi-ai 传输层截断分类](2026-07-22-pi-ai-transport-truncation-classification.md)让 `classifyPiAiError` 认出了两种传输措辞；把该分类器与 pi-ai 自身维护的暂时性错误模式表（`dist/utils/retry.ts`，大多数条目引用上游 issue）对比后，可以看到它仍遗漏了该表的大部分：网关中断（`upstream connect`、`reset before headers`）、OpenRouter 包装的 `Provider returned error`、不带数字状态的裸 `Service Unavailable`、`closed unexpectedly` 之外的 websocket 断开变体、DNS 失败（`getaddrinfo`、`ENOTFOUND`、`EAI_AGAIN`）、提供方在流中途发出的显式重试指引（"you can retry your request" 一族），以及 gRPC `ResourceExhausted` 节流。每一处遗漏都会落入不可重试的 `PI_AI_ERROR`，使一次可恢复的失败永久终结该轮次——正是前一条 note 只针对两种措辞修复过的同一缺陷。

这种漂移是结构性的，而非偶然：适配器固定关闭 pi-ai 的内部重试（`maxRetries: 0`；一次适配器调用就是一次线上尝试），因此 pi-ai 的模式表在运行时不起任何作用。由这个分类器独自决定这些措辞的可重试性，却没有任何机制把它与它实际取代的上游列表绑定在一起。

## Decision

- `classifyPiAiError` 镜像 pi-ai 的暂时性模式表：
  - `TRANSPORT`：`upstream connect`、`reset before headers`、`provider returned error`、泛化的 `websocket closed/error`、DNS 失败（`getaddrinfo`、`ENOTFOUND`、`EAI_AGAIN`），以及 OpenAI Responses 与 Bedrock 在流中断时于流中发出的重试指引短语。
  - `SERVER`：裸 `service unavailable` 与既有数字状态分支并列。
  - `RATE_LIMIT`：gRPC `ResourceExhausted` / `RESOURCE_EXHAUSTED` 节流。
- 优先级保持不变：特定分支（auth 数字、配额文本、429、413/400、5xx 数字、timeout）先行运行，因此携带状态数字或配额措辞的包装消息仍保留其精确 code，而 pi-ai 通用的 `Provider returned an error stop reason` 兜底文案仍不分类。
- `llm-pi-ai` 的两份 README 都写明了这一跟随关系及其原因（`maxRetries: 0`）。

## Alternatives considered

**改为调用 pi-ai 导出的 `isRetryableAssistantError`，而不是复制措辞。** 不采纳：它只回答"可否重试"这一布尔问题，而 harness 需要具体 code（`TRANSPORT` 还是 `RATE_LIMIT` 还是 `QUOTA`），llm-retry 的准入、UI 渲染和诊断都路由在该 code 上；无论怎样做，code 归类都需要我们自己的匹配。它的不可重试订阅限额措辞在这里已由 `isQuotaExceededError` 映射为终止型 `QUOTA` 覆盖。

**把 `PI_AI_ERROR` 设为可重试以吸收未来的措辞漂移。** 前一条 note 已否决，现在仍然否决：该兜底类别容纳真正永久性的失败，默认可重试的 code 只会徒劳重复它们。

**在上游修复根因（在被扁平化之前保留原始 Error）。** 这仍是分类器 `XXX(pi-ai upstream)` 注释所指的持久终态，但在本仓库内无法落地；镜像措辞表在其落地前收窄影响面。

## Consequences

- pi-ai 模式表中的每个暂时性失败家族如今都在默认重试策略下恢复，而不是让轮次失败。
- 对等关系靠手工维护：上游新增措辞仍需更新 harness 的模式，但被跟随的面已是整张维护中的表，而非逐事故的单点补充。
- 过度匹配保持有界：新分支只会看到终止错误消息（绝不接触模型输出），运行在特定 code 检查之后，且每个短语命名的都是传输条件而非对响应内容的判断。
