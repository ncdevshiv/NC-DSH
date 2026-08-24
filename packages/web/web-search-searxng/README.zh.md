# @deepseek-ai/dsh-web-search-searxng

[English](README.md) | 中文

由 [SearXNG](https://docs.searxng.org) 支持的 `WebSearchProvider`，用于 harness [web 能力 seam](../web/README.md)（`ctx.web`）。它调用 SearXNG 实例的 JSON 搜索 API（`GET /search?q=<query>&format=json`），把聚合的 `results[]` 映射为 seam 规范化的 `WebSearchResult`。

这是一个**实现**包：它向 `ctx.web` 注册提供方，不拥有 `ctx.web` 键，也不注册面向模型的工具（后者属于 `@deepseek-ai/dsh-tool-web`）。与 `@deepseek-ai/dsh-llm-deepseek` 一样，它是函数／命名空间插件（`inject: ['web']`），负责注册后端，而非默认导出服务。

## 配置

| 配置键 | 默认值 | 含义 |
|---|---|---|
| `baseURL` | `$SEARXNG_BASE_URL` | 实例基址 URL；追加 `/search`。实例由使用者自建，因此没有内置默认值；为空或无法解析时提供方不可用。 |
| `username` | （空） | 位于带认证反向代理之后的实例使用的基本认证用户名。 |
| `password` | （空） | 基本认证密码；必须与 `username` 成对设置——只配置一半时提供方不可用。 |

配置的凭据对必须可按 Latin-1 编码，因为 HTTP 基本认证通过 `btoa` 传输 `user:pass`；超出该范围的字符会在插件加载时失败，而不是表现为逐次搜索的提供方错误。

```yaml
- id: web-search-searxng
  name: '@deepseek-ai/dsh-web-search-searxng'
  config:
    baseURL: !!js process.env.SEARXNG_BASE_URL
```

## 映射

SearXNG 把上游引擎聚合为扁平 `results[]`。每个 `url` 非空白的条目映射为 `WebSearchSource`：`url` ← `url`、`title` ← `title`、`snippet` ← `content`（空白时省略）、`publishedAt` ← `publishedDate`；没有 url 的条目会被跳过。非空白的 `answers[]` 即时答案以 `\n\n` 连接为 `content`；除此之外 SearXNG 不生成任何内容，因此其余情况省略 `content`。SearXNG 的 JSON API 没有结果数控制参数，所以 `maxResults` 只由 seam 强制执行（截断 `sources[]` 并置位 `truncated`）。提供方失败（HTTP 错误如 JSON 格式被禁用、网络失败、响应体无法解析或结构不符）以 `WebError` `WEB_PROVIDER_ERROR` 呈现；中止请求以 `WEB_ABORTED` 呈现。HTTP 重定向会在访问 `Location` 指向的目标之前被拒绝，并以 `WEB_PROVIDER_ERROR` 呈现。

## 模型体验

通过 [`dsh-tool-web`](../tool-web/README.md) 间接影响；该工具保留此提供方经 `maxResults` 限制的 URL、标题、snippet 与发布日期以及连接后的即时答案，或将确切的错误消息 `SearXNG search aborted`、`SearXNG search request failed: <error>` 和 `SearXNG returned an unprocessable response body: <error>` 置于消费方的错误包装层内；提供方私有字段（引擎名、infobox）不进入上下文。

#### KV Cache 影响

不会直接导致 KV Cache 失效；请求前缀变更由上述消费方负责。

## 已知限制与暂缓事项

- **预期使用私有实例**：公共 SearXNG 实例通常禁用 `format=json`（HTTP 403），请把 `baseURL` 指向自己掌控的实例。
- **上游引擎失败原样呈现**：引擎的 CAPTCHA 和限流会以 `WEB_PROVIDER_ERROR` 消息进入工具结果；SearXNG 聚合引擎，但无法把它们的挑战从 JSON 载荷中隐藏。
- **没有结果数控制**：SearXNG 的 JSON API 没有 count 参数，所以 `maxResults` 只能由 seam 事后截断来强制执行。
- **除即时答案外没有生成答案**：`content` 只承载 SearXNG 的即时答案；没有可供映射的 LLM 生成摘要。
- **只公开 `baseURL`／`username`／`password`**：SearXNG 的查询控制项（分类、语言、时间范围、安全搜索）等待提供方无关的 Service Definition 字段（见 [seam Agent Note](../../../.agents/notes/implemented/architecture/2026-06-24-web-capability-seam.md)）。
- **按错误形状分类中止**：只有 `DOMException` 且名为 `AbortError` 时才映射为 `WEB_ABORTED`；携带自定义原因的中止（例如 `dsh-timeout` 的 `TimeoutReason`）会呈现为 `WEB_PROVIDER_ERROR`。
