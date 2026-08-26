# Agent Note: 提供方错误形状 — 结构化事实在 SDK 扁平化之后仍然存活

Status: implemented

[English](2026-08-25-provider-error-shape.md) | 中文

## 问题

提供方 SDK 在任何 harness 层看到结构化事实之前，就把 HTTP 失败折叠成一个展示字符串。Anthropic SDK 的 `APIError.makeMessage` 渲染 `${status} ${error.message}`，pi-ai 的共享格式化器渲染 `${status}: ${JSON.stringify(body)}`；pi-ai 随后把每个被捕获的错误扁平化进 assistant 消息的 `errorMessage`，丢弃 SDK 的 `status`、结构化 `type`、解析后的 body 和请求 id。一个以状态码 500 应答 `{"error":{"type":"error","message":"Internal server error"}}` 的网关，最终以原始扁平串 `500: {"type":"error","message":"Internal server error"}` 出现在 harness 的重试 UI 中——用户必须从失败标签里解析出 JSON 才能知道发生了什么，而任何消费方想按状态码或提供方类型分支，都只能对展示文本做正则匹配。

## 决策

**适配器负责结构化事实的恢复；`LlmFailure` 原样承载这些事实；`message` 保持人类可读。** 三条规则：

1. **在最早的自有边界处恢复。** 当某个库在我们看到错误之前就将其扁平化时，拥有该边界的适配器应把确定性的扁平串解析回结构化事实，而不是转发原始扁平串。`dsh-llm-pi-ai` 的 `flat-error.ts` 识别两种扁平形状（`<status> <payload>` 与 `<status>: <payload>`），payload 是 JSON 时就解析，并从任一嵌套包络中提取 `message`/`type`/`code`。无法恢复的文本原样通过——字符串没有陈述的东西绝不猜测。
2. **统一经由 seam 的唯一分类器。** `dsh-llm` 的 `classifyHttpStatus(status, detail)` 是唯一的状态到 code 映射；适配器把恢复的事实喂给它，而不是各自维护副本。基于措辞的分类仅作为没有可恢复状态码时的兜底保留。
3. **展示字段彼此独立，永不拼接。** 聊天重试披露区把延迟、原因、HTTP 状态、提供方类型和请求 id 渲染成不同的行；失败消息里永远不嵌入自己的状态前缀或 body JSON。

同样的纪律适用于 web seam：`dsh-web` 的共享 `readErrorBody`/`parseErrorBody`/`throwProviderHttpError` 取代了四份各自的 body 解析流程，把读取限制在 16 KB，非 JSON body 只引用第一行，并把 `status`/`providerType` 附加到 `WebError` 上。

## 后果

`LlmFailure` 新增可选字段 `providerType`（在 `LlmError`、持久不变量与规范化边界均验证 ≤128 字符；纯增量，因此不提升会话格式版本）。DeepSeek 直连适配器同时读取 OpenAI 包络与顶层包络两种形状，并为非 JSON body 附加有界的片段。回归测试端到端地钉住所报告的场景：适配器测试证明扁平串恢复为 `{message: 'Internal server error', code: 'SERVER', status: 500, providerType: 'error'}`，重试测试把完整事实集钉进持久的 `llm/retry` 事件，循环测试把终态 `turn/end` 的保留也钉住。未新增独立的日志行：持久事件就是可查询的记录，平行的日志通道只会重复它们。

## 已否决的替代方案

- **在 pi-ai 内部修复扁平化** — 否决：该行由 vendored 上游拥有；规避必须落在 harness 能编译的位置。
- **用更丰富的措辞表做正则分类** — 否决：措辞会随 SDK 版本变化；从明确的状态码解析出的结构是可证明的，措辞不是。
- **把原始 body JSON 放到 failure 上** — 否决：敌意或超大 body 会进入持久存储与 UI 标签；有界片段已覆盖诊断需求。

相关：[有界 LLM 请求恢复](2026-06-21-bounded-llm-request-recovery.md)（本文扩展的 `LlmFailure` 契约）、[pi-ai 响应元数据捕获](../bug-fix/2026-08-23-pi-ai-response-metadata-capture.md)（与此互补的响应边界事实）。
