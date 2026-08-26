# Agent Note: 基于 ai-sidecar 子进程的单一 LLM 适配器

Status: implemented

[English](2026-08-25-single-llm-adapter-via-ai-sdk.md) | 中文

> 部分取代[双 LLM 适配器](2026-06-13-twin-llm-adapters.md)与[按提供方路由的 LLM 适配器](2026-07-14-provider-routed-llm-adapters.md)：harness 再次只交付一个适配器实现，其取代的按路由传输协议 profile 已被移除。[强制应用归因头](2026-06-21-mandatory-app-attribution-headers.md)不再约束由独立进程拥有传输层的适配器。

## 问题

两个自研适配器包共同拥有提供方协议层：`dsh-llm-deepseek`（直连 DeepSeek HTTP）与 `dsh-llm-pi-ai`（经 `@earendil-works/pi-ai` 库的多提供方支持）。每一项新提供方能力要么落两次，要么经由按路由的 `api` 协议字段、compat 开关 profile（`PiAiCompatProfile`）以及重复多提供方 SDK 已有维护成果的自研 SSE 解析。该第三方依赖还把它的发布节奏钉在了 harness 的模型行为上。AI SDK Rust 工作区（多提供方注册表，含原生 Anthropic 与 Gemini 客户端加一个 OpenAI 兼容客户端）已经存在，但没有暴露 Node harness 可以驱动的进程接口。

## 决策

唯一的包 `@deepseek-ai/dsh-llm-ai-sdk` 为每条已注册路由实现 `LlmAdapter`；harness 中没有其他包打开提供方连接。它通过一个长期存活的 `ai-sidecar` 子进程驱动 AI SDK，子进程在 stdio 上讲换行分隔的 JSON-RPC 2.0（协议版本 1）：`initialize`、`configure`（整体提供方世代替换）、`provider.list`、`model.list`、`model.discover`、`chat.stream`，以及 `chat/event`/`chat/done`/`stream.cancel` 通知。sidecar 的真源在 `crates/ai-sidecar`；部署用 `llm-ai-sdk.binaryPath` 或 `$DSH_AI_SDK_SIDECAR` 选择可执行文件，路径处处未设置时第一个请求即以 `CONFIG` 失败。

路由 id 在 sidecar 内选择传输协议：`anthropic` 与 `google` 是原生的；`openai`、`openrouter`、`ollama` 是 OpenAI 兼容并带内置默认端点；其他任意 id 是 OpenAI 兼容、指向其配置的 `baseURL`。profile 的可选 `api` 字段显式覆盖该推导（[模型发现与路由协议](../feature/2026-08-26-model-discovery-and-route-dialects.md)）。省略 `providers` 保留默认 `deepseek-official` 路由并指向公共端点。

连接事实保持不冻结：`resolveAdapterOptions()` 每次操作重新解析分层配置一次，凭据经 `ctx.credentials`（然后是可信环境层）按请求解析，且只在已解析凭据/端点集合变化时适配器才向子进程推送一个 `configure` 世代——进行中的流保持其启动时的事实。设置热替换经既有 `installSettingsSection` seam 运行，命名空间为 `llm-ai-sdk`；违反超出 schema 约束的无效快照保留最后一份有效事实并记录一次日志。旧包、其组合行与 `.github/workflows/pi-ai-provider-e2e.yml` 均已删除；真实提供方覆盖移至 `packages/llm/llm-ai-sdk/tests/adapter.e2e.ts`，无 `DSH_AI_SDK_SIDECAR` 时自行跳过。


## 备选方案

**保留双适配器，把能力收敛到二者交集。** 否决：每项新的提供方能力都要落两遍，或藏在一个兼容开关 profile 之后，且第三方依赖的发布节奏直接耦合 harness 的模型行为——正是本决策删除的重复。

**经进程内 Node↔Rust 桥驱动 AI SDK，而非 sidecar 子进程。** 本轮否决：原生插件会把每个开发机与部署的构建都耦合到 Rust 工具链，而 stdio JSON-RPC 子进程让边界保持为一个任何部署都可重建或替换的 release 二进制。

**保留按路由 `api` 协议字段作为协议选择器。** 被[模型发现与路由协议](../feature/2026-08-26-model-discovery-and-route-dialects.md)部分取代：该字段以显式覆盖的形式回到单适配器的 profile 上，路由 id 推导仍是默认。
## 后果

- bundle 行形状从两个带按路由协议/compat 字段的适配器条目变为一个条目，其 `providers` 字典承载 `displayName`/`apiKeyEnv`/`baseURL`/`api`/`models`/上限/力度；`binaryPath` 是每份配置都必须提供的唯一新部署事实。
- 提供方 HTTP 头是 sidecar 的属性：如今的请求不携带 `attributionHeaders()`、`x-deepseek-harness-user-id` 或压缩标记。未来的任何头需求都落在 sidecar 协议中。
- 推理力度在线路上收敛（`max` → `high`），因为协议只定义三个级别；显式 `off` 省略线路字段而非强制关闭思考；先前轮次的思考以普通助手文本回传，因此提供方原生思考签名无法往返。
- 超过路由 `maxRequestImageBytes` 的图片负载直接拒绝请求，而不是降级为占位符。
- web Models 页面保留密钥管理、模型目录编辑、自定义提供方创建与实时端点探询；其启用方式由[模型发现与路由协议](../feature/2026-08-26-model-discovery-and-route-dialects.md)承载。