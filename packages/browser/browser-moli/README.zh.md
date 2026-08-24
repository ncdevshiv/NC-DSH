# @deepseek-ai/dsh-browser-moli

[English](README.md) | 中文

[moli](https://github.com/lexmount/moli) 后端 `BrowserProvider`，面向 harness 的[浏览器能力 seam](../browser/README.md)（`ctx.browser`）。每次启动会在保留的临时端口上派生一个隔离的 `moli serve` 进程，等待其 HTTP CDP 端点应答，通过 WebSocket 附着到一个页面 target，并交付可导航、读取渲染状态、按 CSS 选择器交互、捕获 PNG 截图的 [`BrowserSession`](../browser/README.md)。

这是一个**实现**包：它向 `ctx.browser` 注册 provider，不拥有该键，也不注册模型侧工具（那是 `@deepseek-ai/dsh-tool-browser`）。与各 web provider 一样，它是函数/命名空间插件（`inject: ['browser']`）。

## 工作方式

- 可用性是一次记忆化的本地 `moli --version` 探测——已挂载的 provider 在 `$MOLI_BINARY`（或 `PATH`）可解析之前保持休眠。
- 一个会话 = 一个 serve 进程：启动之间状态隔离；每个会话一个生命周期控制器，同时拥有子进程与 WebSocket，任何路径的 `close()` 都会将其拆除。关闭连接会立即结算所有阻塞中的命令与事件等待，因此 `close()` 绝不会等待期限计时器走完。崩溃或硬退出遗留的 serve 进程会在 Node 同步退出阶段被强制终止。
- 导航在承载任何 CDP 流量之前按 seam 契约校验目标（只接受绝对 `http:`/`https:`，否则报 `BROWSER_INVALID_URL`）；页内执行失败以 `BROWSER_EVALUATION_FAILED` 呈现，而不是返回空页面状态。
- 每个操作都把调用方的 `AbortSignal` 贯穿到 CDP 层：取消会立即以 `BROWSER_ABORTED` 拒绝，包括导航期间与启动发现阶段。
- CDP 客户端是包内最小实现（按 id 关联命令、事件等待），跑在 Node 全局 WebSocket 上；不新增自动化依赖。
- 操作在会话内串行；页面文本走有界的 `Runtime.evaluate` 读取；交互执行小型 DOM 脚本；截图走 `Page.captureScreenshot`。

## Config

| 键 | 默认 | 含义 |
|---|---|---|
| `binaryPath` | `$MOLI_BINARY` ?? `'moli'` | moli 可执行文件。探测时无法解析则 provider 不可用。 |
| `startupTimeoutMs` | `15_000` | 单个会话服务端启动就绪轮询的预算。 |
| `navigationTimeoutMs` | `30_000` | 单次页面导航的预算。 |
| `cdpTimeoutMs` | `30_000` | 导航之外单条 CDP 命令或事件等待（`Runtime.evaluate`、截图）的预算。 |
| `maxContentChars` | `100_000` | 返回页面文本内容的字符上限。 |
| `settleMs` | `150` | DOM 交互之后、读取状态之前的 settle 延迟。 |
| `probeTimeoutMs` | `5_000` | 一次性可用性探测的预算。 |
| `pollEveryMs` | `100` | 就绪轮询间隔。 |
| `extraServeArgs` | `[]` | 原样追加到 `moli serve` 调用后的额外 argv（用于覆盖旗标）。 |

```yaml
- id: browser-moli
  name: '@deepseek-ai/dsh-browser-moli'
  config:
    binaryPath: !!js process.env.MOLI_BINARY
```

## Model Experience

间接地，通过 [`dsh-tool-browser`](../tool-browser/README.md)：它拥有稳定的模型侧名称、schema、提示词指引与呈现，provider 失败以结构化 `BrowserError` 呈现。

#### KV Cache effect

无直接失效；由具名消费者拥有任何请求前缀变化。

## Known Limitations and Deferred Work

- **依赖外部 moli 二进制** —— 不随包分发；按项目 release 安装。
- **默认 `--cdp-port` 旗标拼写是一个假设** —— moli 的 serve 旗标在此不受契约约束；部署可通过 `extraServeArgs` 纠正调用而无需改代码。
- **仅支持 CSS 选择器交互** —— 基于元素引用的操作等待快照词汇；点击是 DOM 派发而非坐标输入。
- **仅软件渲染** —— 截图遵循 moli 的布局策略（按需软件绘制）；上游也不追求与 Chrome 的像素级一致。
