# 配置模型

[English](providers.md) | 中文

本指南假设你已通过[根 README](../../../README.md#run) 启动 Web UI。模型更改在下一次请求时生效，无需重启服务器。

每一条提供方路由都由驱动 `ai-sidecar` 伴生进程的同一个适配器服务；路由 id 及其传输协议见[提供方路由](#provider-routes)。

<a id="set-up-the-sidecar-binary"></a>

## 安装 sidecar 二进制

模型请求需要 `ai-sidecar` 可执行文件。在 `binaryPath`（cordis.yml 或 `llm-ai-sdk` 设置分区）中设置绝对路径，或在启动环境中导出 `DSH_AI_SDK_SIDECAR`。路径处处未设置时，第一个请求以 `CONFIG` 失败并列出所有配置入口；Models 页面保持可达，可在启动后再配置该路径。

## 配置 DeepSeek

打开 **Settings → Models**。DeepSeek 卡片提供一个 API 密钥字段；输入密钥并保存。

![Models 页面：每张提供方卡片各有一个密钥字段](providers-models-page.png)

密钥是只写的。保存后页面收到的只是脱敏描述符，绝不会是字面密文。密钥存储在 `$DSH_HOME/.credentials.yaml` 中，设置里只保留其凭据引用。

默认的 `deepseek-official` 路由无需任何配置即可存在：它通过 `DEEPSEEK_API_KEY` 凭据引用服务 DeepSeek 公共端点，因此对默认安装而言，输入密钥是唯一的准备工作。

<a id="provider-routes"></a>

## 提供方路由

路由是 `llm-ai-sdk` 提供方映射中的一个命名条目。每条已声明的路由在 Models 页面上呈现为一张卡片；其显示名称、基础 URL、密钥与模型目录都可在此编辑。

路由 id 决定请求的说话方式：

| 路由 id | 传输协议 | `baseURL` |
|---|---|---|
| `anthropic` | 原生 Anthropic | 可省略 |
| `google` | 原生 Gemini | 可省略 |
| `openai` | OpenAI 兼容 | 可省略（`https://api.openai.com/v1`） |
| `openrouter` | OpenAI 兼容 | 可省略（`https://openrouter.ai/api/v1`） |
| `ollama` | OpenAI 兼容 | 可省略（`http://localhost:11434/v1`） |
| 其他任意 id | OpenAI 兼容 | 必填 |

在 `$DSH_HOME/settings.yaml` 的 `llm-ai-sdk` 分区中声明更多路由：

```yaml
llm-ai-sdk:
  providers:
    anthropic:
      apiKeyEnv: ANTHROPIC_API_KEY
      models:
        - id: claude-sonnet
          contextWindow: 200000
    my-gateway:
      displayName: Acme gateway
      apiKeyEnv: GATEWAY_API_KEY
      baseURL: https://gateway.example/v1
      models:
        - id: legacy-chat
        - id: vision-preview
          inputModalities: [text, image]
```

省略 `apiKeyEnv` 时解析约定引用 `<ROUTE>_API_KEY`。自定义 OpenAI 兼容网关需要完整端点基址（含版本路径，通常为 `/v1`）。已保存会话所用的路由 id 是永久的：请求、会话日志、模型默认值与凭据引用都使用它。要重命名路由，请声明新 id、把工作迁移过去，再删除旧行。

### 模型目录

每个 profile 的 `models` 列表仅作参考：未列出的模型 id 原样透传，选择器展示的正是列出的条目，并以 `name` 为标签（缺省为 id）。`inputModalities` 省略即纯文本；`[text, image]` 才会在该模型上放行图片输入。

### 图片输入

向纯文本模型附加图片会在发送前被拒绝并指名模型。累计 base64 图片负载超过该路由 `maxRequestImageBytes`（默认 20 MiB）的请求同样会被拒绝；请调低上限或减少图片——不会通过替换或丢弃内容来迁就请求。

### 推理控制

每条路由发布其可选推理力度（默认 `off`、`low`、`high`、`max`）及默认力度（`high`）。composer 的模型选择器提供该路由的级别；未显式选择的请求使用路由默认值。

## 选择模型

已配置的提供方出现在模型选择器中。选择一个模型同时会将其设为新会话的默认值。已经发出过请求的会话保留其自身日志中记录的模型。

如果保存的默认值指向已被删除的提供方，composer 会显示 **Select model** 并阻止输入，直到选定另一个模型。

## 故障排除

- **`CONFIG`** —— 未配置 sidecar 二进制。参见[安装 sidecar 二进制](#set-up-the-sidecar-binary)。
- **`MISSING_CREDENTIAL`** —— 通过 Models 页面存储提供方密钥，或提供引用的环境变量。
- **`NO_ADAPTER`** —— 会话指向的路由已不存在。选择一个已配置的模型，或以同一 id 重新声明该路由。
- **请求到达网关但全部被拒** —— 自定义路由说的是 OpenAI chat completions；检查基础 URL 是否包含版本路径、端点是否服务 `/chat/completions`。原生 Anthropic 或 Gemini 端点必须使用路由 id `anthropic` 或 `google`。
- **图片在发送前被拒绝** —— 模型未声明图片模态。在其 profile 中为模型添加 `inputModalities: [text, image]`。
- **`TIMEOUT`** —— 流在等待提供方时空闲超过了 `streamIdleTimeoutMs`（默认五分钟）；检查端点健康状况，或为慢端点调高预算。
- **提供方拒绝携带图片的请求** —— 模型声明的图片能力其端点并不真正支持。从该模型的 `inputModalities` 中移除 `image`，然后开启新会话：附件图片已留在会话日志中，同样的请求会反复出现，直到会话越过它。

## 高级配置

生成的[插件配置目录](../../config-catalog.md#deepseek-aidsh-llm-ai-sdk)列出本插件支持的每一个字段和默认值，它派生自源码，不会落后于适配器实际接受的内容。[dsh-llm-ai-sdk](../../../packages/llm/llm-ai-sdk/README.md) 参考文档负责直接的 `settings.yaml` 配置、路由解析、推理控制、凭据、sidecar 生命周期与适配器错误。