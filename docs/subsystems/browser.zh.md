# 浏览器自动化

[English](browser.md) | 中文

浏览器自动化 seam 是单一 `ctx.browser` 服务上的能力 seam，拆分到多个包：Service Definition（[dsh-browser](../../packages/browser/browser)，`ctx.browser` + 提供方注册表）、Service Provider（[dsh-browser-moli](../../packages/browser/browser-moli)，通过 CDP 驱动本地 [moli](https://github.com/lexmount/moli) 无头浏览器）与 Consumer（[dsh-tool-browser](../../packages/browser/tool-browser)，即 `browser_*` 工具 schema）。浏览器是一项可选能力：其词汇定义在此而非 [core.md](core.md)。更换提供方不会改变模型请求导航、交互或截图的方式。

来源：[`packages/browser/browser/src/types.ts`](../../packages/browser/browser/src/types.ts)

## 会话有状态且串行

`BrowserSession` 在一次 agent 运行内跨调用持久：导航、Cookie 与存储会延续，因此 `browser_snapshot` 能看到上一个动作产生的状态。消费方（tool-browser）为每个上下文持有一条共享会话，经 `ctx.browser.launch()` 惰性启动，把每个操作排在前一个之后串行执行，并在其 fiber 释放时关闭会话。Provider 实现拥有底层的进程或连接；moli 的每次启动都会派生一个隔离的 `moli serve` 子进程。

## 请求与结果

交互请求携带 CSS 选择器（`click`、带可选回车提交的 `type`）；`navigate` 接受绝对 HTTP(S) URL；`screenshot` 截取视口，或在 `fullPage` 下截取整页并输出 PNG 字节。每个交互都解析为 `BrowserPageState`：最终 URL 加可选标题与有界文本内容。提供方限制内容长度；消费方限制渲染输出。

## 可用性与选择

Provider 的 `available()` 是廉价的本地检查（对 moli 是记忆化的 `--version` 探测），绝不会启动常驻进程。选择语义与 [web](web.md) 一致：已配置的 id 优先；未配置时恰好一个可用提供方自动入选；歧义与不可用以结构化代码拒绝，而不是按注册顺序挑选。

## 错误

`BrowserError extends HarnessError` 携带开放式字符串 `code`，与 `WebError` 一致。seam 中立代码来自共享运行时约定：`BROWSER_PROVIDER_UNAVAILABLE`、`BROWSER_PROVIDER_CONFIGURED_MISSING`、`BROWSER_PROVIDER_CONFIGURED_UNAVAILABLE`、`BROWSER_PROVIDER_AMBIGUOUS`、`BROWSER_DUPLICATE_PROVIDER`、`BROWSER_ABORTED`、`BROWSER_SESSION_CLOSED`。实现自有代码涵盖启动超时、导航超时、无效导航目标、元素缺失、截取失败、超大截图与页内执行失败；消费方必须容忍未知代码。

<!-- BEGIN GENERATED cordis-surface (gen-cordis-catalog.ts) — do not edit between markers -->

<a id="cordis-surface"></a>

## Cordis API

Generated from source by `scripts/gen-cordis-catalog.ts` (verified fresh by `bun run verify-cordis-catalog` in doc-sync; regenerate with `bun run gen-cordis-catalog`) — this section is byte-identical in both language sides of the page. Signature blocks use a `ts cordis-catalog` fence and keep the original source JSDoc; dispatch modes are defined in the [primer](../cordis-primer.md#dispatch-modes), and the framework-inherited `ctx` API lives in [cordis-api/inherited.md](../cordis-api/inherited.md).

<a id="ctxbrowser--browserruntime"></a>

### `ctx.browser` — `BrowserRuntime`

The browser-automation service. Registered as `ctx.browser` (one instance per context).

Selection semantics (resolved at launch time, never order-dependent):

- A configured id that is registered and `available()` → that provider.
- A configured id not registered → `BROWSER_PROVIDER_CONFIGURED_MISSING`.
- A configured id registered but unavailable → `BROWSER_PROVIDER_CONFIGURED_UNAVAILABLE`.
- No id configured, exactly one registered usable provider → that provider.
- No id configured, multiple usable providers → `BROWSER_PROVIDER_AMBIGUOUS`.
- No id configured, no usable provider → `BROWSER_PROVIDER_UNAVAILABLE`.

```ts cordis-catalog
/**
 * Register a browser provider. Throws {@link BrowserError} `BROWSER_DUPLICATE_PROVIDER`
 * if its id is already registered. Returns a disposer; disposed with the calling fiber.
 * @param provider - the provider; its `id` is the registry key.
 * @returns the disposer that unregisters the provider.
 */
registerProvider(provider: BrowserProvider): () => void

/**
 * Launch a session through the selected provider. Resolves the provider at call time with the
 * selection rules above; throws {@link BrowserError} when no provider can run. The caller owns
 * the returned session and must eventually call its `close()`.
 * @param signal - optional cancellation signal for the startup phase.
 * @returns the launched session.
 */
async launch(signal?: AbortSignal): Promise<BrowserSession>
```

Source: [`packages/browser/browser/src/index.ts:68`](../../packages/browser/browser/src/index.ts)
<!-- END GENERATED cordis-surface -->
