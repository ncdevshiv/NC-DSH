# @deepseek-ai/dsh-llm-ai-sdk

[English](README.md) | 中文

harness 唯一的 LLM（大语言模型）适配器：单个 `AiSdkAdapter` 实例经由一个长期存活的 `ai-sidecar` 子进程服务每一条已配置的提供方路由——DeepSeek 官方、OpenRouter、Ollama、Anthropic、Gemini、任何 OpenAI 兼容网关。sidecar 通过 stdio 上的换行分隔 JSON-RPC 2.0（协议版本 1）通信，并拥有全部提供方协议实现；harness 中没有其他包会打开提供方连接。

适配器只负责传输。路由经每次操作解析一次的 thunk 到达，bearer 密钥按请求解析，且 sidecar 只在已解析的凭据/端点世代变化时重新配置，因此进行中的流保持其启动时的事实。包根入口导出 Cordis 插件约定（`apply`、`resolveAdapterOptions`）以及客户端、适配器与转换 helper；sidecar 二进制本身是外部组件，完全由配置选择。

## 配置

| 键 | 类型 | 默认值 | 说明 |
|---|---|---|---|
| `binaryPath` | string | `$DSH_AI_SDK_SIDECAR` | `ai-sidecar` 可执行文件的绝对路径。处处未设置时，第一个请求以 `CONFIG` 失败并给出设置指引；web Models 页面保持可达，可在启动后配置该路径。 |
| `providers` | `ProviderProfile` 字典 | 默认 DeepSeek 路由 | 以路由 id 为键的路由集合；省略时保留针对公共端点、使用 `DEEPSEEK_API_KEY` 引用的 `deepseek-official`。 |
| `streamIdleTimeoutMs` | number | `300000` | 一路流读取未完成期间，提供方侧的最大空闲时间。 |
| `retryPolicy` | `RetryPolicyConfig` | normal 模式默认值 | 共享的模型请求重试策略，注册为提供方元数据并由 `dsh-llm-retry` 执行。 |

每个 `ProviderProfile` 接受 `displayName`、`apiKeyEnv`、`baseURL`、`api`、`models`、`maxTokens`（默认 256,000）、`defaultContextWindow`（默认 1,000,000）、`reasoningEfforts`、`reasoningEffort`（默认 `high`）和 `maxRequestImageBytes`（默认 20 MiB）。省略 `apiKeyEnv` 时解析引用 `<ROUTE>_API_KEY`（路由键大写）。

`api` 显式命名传输协议——`openai-compatible`、`anthropic` 或 `google` 三者之一。省略时按下表由路由 id 推导；当自定义端点讲一种其 URL 无法识别的原生格式（自有域名上的 Anthropic 格式中继），或需要为原本会被原生解析的路由 id 钉住 OpenAI 兼容族时，声明它。

```yaml
- id: llm-ai-sdk
  name: '@deepseek-ai/dsh-llm-ai-sdk'
  config:
    binaryPath: /opt/ai-sdk/bin/ai-sidecar # or $DSH_AI_SDK_SIDECAR in the launching environment
    providers:
      deepseek-official:                   # default route kept explicit here
        apiKeyEnv: DEEPSEEK_API_KEY
        baseURL: https://api.deepseek.com
        models:
          - id: deepseek-v4-flash
            name: DeepSeek-V4-Flash
          - id: deepseek-vision
            inputModalities: [text, image]
      anthropic:
        apiKeyEnv: ANTHROPIC_API_KEY       # baseURL omitted: native SDK default applies
        models:
          - id: claude-sonnet
            contextWindow: 200000
      my-gateway:                          # custom OpenAI-compatible gateway
        apiKeyEnv: MY_GATEWAY_KEY
        baseURL: https://gateway.internal/v1
        maxRequestImageBytes: 10485760
      acme-relay:                          # native Anthropic format behind a custom URL
        api: anthropic
        apiKeyEnv: ACME_RELAY_KEY
        baseURL: https://relay.acme.internal
```

## 提供方路由

路由 id 同时选择组合条目与 sidecar 应用的传输协议；显式 `api` 覆盖该推导：

| 路由 id | 传输协议 | `baseURL` |
|---|---|---|
| `anthropic` | 原生 Anthropic | 可省略 |
| `google` | 原生 Gemini | 可省略 |
| `openai` | OpenAI 兼容 | 可省略（`https://api.openai.com/v1`） |
| `openrouter` | OpenAI 兼容 | 可省略（`https://openrouter.ai/api/v1`） |
| `ollama` | OpenAI 兼容 | 可省略（`http://localhost:11434/v1`） |
| 其他任意 id | OpenAI 兼容 | 必填 |

catalog 条目仅作参考：未列出的模型 id 原样透传，`ctx.llm.listModels(route)` 将它们暴露给 ACP 编辑器与 web 选择器等发现类消费者。条目名省略时默认为其 id，`inputModalities` 省略即视为纯文本——声明 `[text, image]` 才会在该模型上放行图片输入；未编目的端点按纯文本处理，而不是假设其具备能力。

## 模型发现

插件服务 `ctx.llm.discoverModels('llm-ai-sdk', request)`，让配置界面得以探询一个仍在草稿中的端点。仅指名既有路由的草稿是在问「你已知道什么」，由该路由的参考 catalog 直接作答，不触碰 sidecar；携带端点（以及可选的、尚未存储的手输密钥）的草稿到达 sidecar 的 `model.discover` 方法，后者为这一次列表调用构建临时 provider，绝不加入已配置世代。协议从草稿显式 `api` 解析，其次是指名路由的 profile，再次是上表的路由 id 推导；无任何端点的 OpenAI 兼容探询在任何网络 I/O 之前以 `CONFIG` 失败。一次性密钥只跨越本次调用，事后不被任何一方持有。

## 请求组装

适配器将组装好的 harness 请求翻译为 sidecar 的 `Message`/`StreamEvent` JSON：系统提示在前，其后是历史记录；工具 schema 以 `name`/`description`/`input_schema` 传递；`temperature`、`max_tokens` 与 `stop` 原样透传。图片块在组装前经 `ctx.attachments` 解析出存储字节并转为 base64 分片，包括嵌套在工具结果内容中的图片。累计 base64 图片负载超过该路由的 `maxRequestImageBytes` 时，请求在任何网络 I/O 之前以 `UNSUPPORTED_CONTENT` 拒绝——该上限是硬上限，不是占位符替换。纯文本模型与未编目模型在凭据解析或附件读取之前就拒绝图片输入。

推理力度按请求解析：省略时由 profile 的 `reasoningEffort` 填充（默认 `high`），因此部署的思考姿态无需每次调用显式命名即可生效。harness 词汇映射到 sidecar 的三个级别——`low` 映射为 `low`，`high` 与 `max` 映射为 `high`；显式 `off` 则完全省略协议字段，由端点自身的默认值决定。路由通过 `ctx.llm.resolveModelInfo` 向选择器界面发布其可选力度。

`streamIdleTimeoutMs` 约束每一路未完成读取（含 sidecar 的上游连接），不计消费者在分片之间耗费的时间；超时抛出 `LlmError('TIMEOUT')`，调用方中止抛出 `LlmError('ABORTED')`。此外每个 JSON-RPC 调用带有固定的 120 秒上限，避免僵死的子进程永久占住一个请求。适配器将配置的重试策略注册为提供方元数据；`dsh-llm-retry` 在持久化的 agent 步骤边界上执行它。

## 动态配置（settings + 凭据）

连接事实不在加载时冻结。`resolveAdapterOptions` 是从原始配置到已验证路由的唯一显式解析步骤，在插件加载时（失败即响）以及每个设置快照首次使用时各调用一次。三个可选 seam 为每次操作供给事实：

- **`ctx.settings`** —— 插件以同一份 `Config` schema 注册 `llm-ai-sdk` 命名空间，并以 cordis.yml 条目作为组合 base，因此用户设置文档中的 `llm-ai-sdk:` 分区可以在不重启的情况下覆盖任意字段；每条路由的切片位于 `providers.<route>`。通过 schema 但违反超出 schema 的约束的快照保留最后一份有效事实并记录失败；组合条目本身仍会使插件加载失败。路由成员关系跟随实时解析并原子替换，观察者永远不会看到空的路由集合。
- **`ctx.credentials`** —— 路由密钥在每次流调用时从提供端点的同一快照解析，因此轮换后的凭据可送达紧接着的下一次请求。配置只携带 `apiKeyEnv`，从不携带字面密钥。处处无密钥的请求以 `MISSING_CREDENTIAL` 失败并列出所有配置入口，此时路由仍保持注册且可浏览。
- **`ctx.attachments`** —— 按请求解析；缺席时以 `UNSUPPORTED_CONTENT` 拒绝图片输入。

当已解析的凭据/端点集合变化时，适配器在下一次请求前向子进程推送一个 `configure` 世代。兄弟路由在其密钥可无异常解析时一并加入该世代；缺少凭据的兄弟路由保持未配置，直到某个请求指名它。每个请求还会在可配置提供方目录（`ctx.llm.listConfigurableProviders()`）中声明自己的路由：provider `<route>`、settings 命名空间 `llm-ai-sdk`、settings 路径 `providers.<route>`。

## Sidecar 生命周期

子进程在首次使用时惰性启动并初始化一次；并发调用者共享同一次启动。流按 `stream_id` 多路复用：`chat.stream` 接受请求，`chat/event` 通知携带扁平事件，`chat/done` 终止流。从迭代器中途退出会发送 `stream.cancel`，使 sidecar 及时中止其上游 HTTP 请求。子进程退出会立即令所有未完成请求与流失败。失败的初始化（请求上限、JSON-RPC 错误）会拆除自己的世代——杀死仍然健康的子进程，而不是让后继者与其并列堆积；销毁（经 Cordis effect）会杀死子进程并拒绝一切仍在途的操作。拆除按世代感知：一个垂死子进程的退出事件不会拆掉它之后启动的后继者。

## 错误

抛出的 `LlmError` 代码：`NO_ADAPTER`（未知路由）、`CONFIG`（未配置 sidecar 二进制）、`MISSING_CREDENTIAL`、`INVALID_CREDENTIAL`（不可用的密钥材料）、`UNSUPPORTED_CONTENT`（纯文本模型上的图片、缺失附件服务或超限负载）、`TIMEOUT` 与 `ABORTED`。终止 `finish` 分片携带的 sidecar 失败按类型化错误种类映射到 harness 词汇：`rate_limit` 映射为 `RATE_LIMIT`，`authentication` 映射为 `INVALID_CREDENTIAL`，`configuration` 映射为 `CONFIG`，`timeout` 映射为 `TIMEOUT`，`cancelled` 映射为 `ABORTED`，`network` 与 `serialization` 映射为 `TRANSPORT`，其余映射为 `PROVIDER`。

## Model Experience
### 每条已配置路由
#### What the model sees

被选中的模型接收 harness 系统提示、消息历史、工具 schema、停止序列与调用配置的 sidecar 消息格式译文，不含适配器撰写的提示文本。具备图片能力的 catalog 条目还会按原顺序收到以 base64 分片表示的用户与工具结果图片。先前助手轮次的思考以普通助手文本回传，受益于看见自己早先推理的模型仍然能收到它。

#### Token effect

提供方分词决定精确的文本与图片 token 输入；sidecar 上报用量计数，缓存命中输入 token 从上报的输入总量中拆出而不是重复计入。在发送前拒收超限请求避免了为被拒负载支付 token；推理增量成为被记录的 reasoning block，其去留决策属于 loop。

#### KV Cache effect

未变化的组装前缀在端点支持时可用于提供方缓存复用，并经 usage 分片的缓存命中计数呈现。路由、模型或上游提示/历史变化可能使复用自第一个变化的 token 起失效；由于先前思考以普通助手文本回传，以原生 reasoning 字段为缓存键的提供方在这些轮次可能无法命中。

## Known Limitations and Deferred Work

- 先前轮次的思考序列化为普通助手文本，丢失提供方原生思考签名；需要原始签名块才能重放思考的端点无法从本翻译中恢复它。
- sidecar 协议只携带三个推理级别（`low`、`medium`、`high`）；请求的 `max` 到达提供方时是 `high`，而 harness 日志保留 `max`。
- 显式力度 `off` 省略协议字段而非强制关闭思考，因此默认开启思考的端点在该请求上仍可能思考。
- 请求不携带应用归因 HTTP 头；头字段的所有权在 sidecar 内部，而它目前不发送任何归因头。
- 用量上报只暴露缓存命中输入 token；此路径上没有 cache-write 指标。
- 流中途的 `error` 事件表现为可见的思考文本而非失败的 finish，因为它不带失败分类。