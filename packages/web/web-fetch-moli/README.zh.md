# @deepseek-ai/dsh-web-fetch-moli

[English](README.md) | 中文

[moli](https://github.com/lexmount/moli) 后端 `WebFetchProvider`，面向 harness 的 [web 能力 seam](../web/README.md)（`ctx.web`）。它通过本地 moli 无头浏览器渲染目标 URL（`fetch --dump markdown`）并返回渲染后的 markdown，使 JavaScript 渲染的页面能返回真实内容，而纯 HTTP GET 只能看到空壳。

这是一个**实现**包：它向 `ctx.web` 注册 provider，不拥有 `ctx.web` 键，也不注册模型侧工具（那是 `@deepseek-ai/dsh-tool-web`）。与 `@deepseek-ai/dsh-web-fetch-http` 一样，它是函数/命名空间插件（`inject: ['web']`）。

## 职责划分

Provider 通过外部二进制负责**安全的资源获取**：URL 校验、子进程传输、中止传播、资源兜底超时和字符上限。`@deepseek-ai/dsh-tool-web` 拥有**呈现**。可用性是一次记忆化的本地 `moli --version` 探测，因此已挂载的 provider 在二进制可解析之前保持休眠。

Provider 的 `timeoutMs` 是面向直接 `ctx.web.fetch()` 调用方与错误部署的资源兜底，不是模型侧工具调用预算。[`dsh-tool-call-timeout-policy`](../../guard/timeout-policy/README.md) 拥有工具调用预算；外层截止先到时本 provider 报告 `WEB_ABORTED`，兜底耗尽则报告 `WEB_FETCH_TIMEOUT`。

## 传输卫生

- 只接受 `http:`/`https:` URL；在任何进程启动前拒绝 URL 内嵌凭证（`WEB_BLOCKED_URL`）与过长/畸形 URL（`WEB_INVALID_URL`，超出配置的 `maxUrlLength`）。调用方的中止信号已经触发时，同样在子进程启动之前拒绝。
- 无 shell 地运行 moli 并把中止传播进子进程。
- 天然是浏览器级：页面 JavaScript 会执行，重定向按浏览器语义原生跟随——[`dsh-web-fetch-http`](../web-fetch-http/README.md) 的仅同源重定向规则在这里不适用。
- 输出渲染后的 markdown，按 seam 的 `kind: 'text'` 正文分类（markdown 就是文本；工具对文本原样透传），直到封闭的 `WebFetchBody` 联合类型增加 markdown 分支。

## Config

| 键 | 默认 | 含义 |
|---|---|---|
| `binaryPath` | `$MOLI_BINARY` ?? `'moli'` | moli 可执行文件：PATH 名或路径。探测时无法解析则 provider 不可用。 |
| `maxUrlLength` | `2048` | 接受的请求 URL 最大字符数。 |
| `maxBodyChars` | `100_000` | 返回 markdown 的最大字符数；超出部分被截断并标记。 |
| `timeoutMs` | `30_000` | Node 定时器范围内的获取超时——面向直接调用方的资源兜底，不是模型侧预算。 |
| `probeTimeoutMs` | `5_000` | 一次性 `--version` 可用性探测的预算。 |

数值上限在插件构造时校验：每个上限必须为正有限数，`timeoutMs` 在 Node 定时器范围内。非法值直接抛出，而不是静默构造出限制荒谬的 provider。

```yaml
- id: web-fetch-moli
  name: '@deepseek-ai/dsh-web-fetch-moli'
  config:
    binaryPath: !!js process.env.MOLI_BINARY
```

## Model Experience

间接地，通过 [`dsh-tool-web`](../tool-web/README.md)：它把本 provider 以 `maxBodyChars` 截断的 markdown 放进其抓取结果包装中并保留 provider 失败，同时重定向、报头与传输机制保持不可见。

#### KV Cache effect

无直接失效；由具名消费者拥有任何请求前缀变化。

## Known Limitations and Deferred Work

- **依赖外部 moli 二进制** —— 不随包分发；按项目 release 安装，不在 `PATH` 时用 `binaryPath`/`$MOLI_BINARY` 指向它。
- **成功时 `statusCode` 恒为 200** —— dump 模式不暴露 HTTP 状态；真正失败的导航以 provider 错误呈现，而非非 2xx 结果。
- **SSRF / 私网保护随 seam 一并延后** —— seam 层的延后在此适用，且 moli 会执行页面 JavaScript 发起自己的请求；其 `--private-network` 策略开关尚未在此暴露。在渲染页面可能触达敏感内网目标的部署中不要启用 fetch。
- **Markdown 借道 `kind: 'text'`**，直到封闭正文联合类型增加 markdown 分支。
