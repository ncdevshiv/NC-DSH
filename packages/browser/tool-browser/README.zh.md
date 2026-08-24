# @deepseek-ai/dsh-tool-browser

[English](README.md) | 中文

面向 [浏览器能力 seam](../browser/README.md)（`ctx.browser`）的模型侧浏览器工具套件——`browser_navigate`、`browser_snapshot`、`browser_click`、`browser_type` 与 `browser_screenshot`。本包只拥有模型侧关注点：工具名、JSON schema、提示词指引、各工具预算与输出渲染；所有浏览器交互都经由一个共享的串行会话（首次使用时惰性启动，fiber 释放时关闭），本包从不导入具体 provider。

## Tools

| 工具 | 参数 | 行为 |
|---|---|---|
| `browser_navigate` | `url`（必填） | 加载 URL 并返回页面状态（`url`、标题、文本内容）。 |
| `browser_snapshot` | — | 不导航地读取当前页面状态。 |
| `browser_click` | `selector`（必填） | 点击第一个 CSS 选择器匹配并返回结果页面状态。 |
| `browser_type` | `selector`、`text`（必填）、`submit?` | 清空后填入首个匹配项，可选追加回车；返回结果页面状态。 |
| `browser_screenshot` | `full_page?` | 将当前页面截取为 PNG 保存到临时文件；返回路径与字节数。 |

每个工具独立注册；产品通过配置禁用某个动作（`{ click: false }` 等）。无论启用哪些工具，套件只贡献一个共享的指引段，因此切换单个动作不会改写提示词文本。

## Config

| 键 | 默认 | 含义 |
|---|---|---|
| `navigate` / `snapshot` / `click` / `typing` / `screenshot` | `true` | 注册对应工具。 |
| `navigationTimeoutMs` | `30000` | `browser_navigate` 的协作预算。 |
| `actionTimeoutMs` | `15000` | 页内动作与截图的协作预算。 |
| `maxOutputChars` | `20000` | 单个工具完整渲染输出的上限。 |

超时以 `ToolDefinition.timeoutMs` 附着并由 [`dsh-tool-call-timeout-policy`](../../guard/timeout-policy/README.md) 强制执行；不存在面向模型的超时参数。

```yaml
- id: tool-browser
  name: '@deepseek-ai/dsh-tool-browser'
```

## Stable registration

已启用的工具在没有可用浏览器 provider 时仍然可见；执行时经 `ctx.browser` 解析 provider，结构化 `BrowserError`（如 `BROWSER_PROVIDER_UNAVAILABLE`）成为可读的错误结果。Provider 选择完全位于 seam 内部。

## Model Experience

### System prompt

#### What the model sees

只要启用了任一浏览器工具，就会注册同一段共享指引，且其文本不随启用集合变化。

##### 浏览器工具指引

```markdown
Use the browser_* tools to drive a real headless browser: browser_navigate loads a URL, browser_snapshot reads the current page state, browser_click and browser_type act on elements matched by CSS selector (use browser_snapshot first to find selectors), and browser_screenshot saves a PNG of the current page and returns its path. The session persists across calls within a conversation.
```

#### Token effect

启用任一浏览器工具时每请求固定段开销。

#### KV Cache effect

启用集合不变时前缀稳定；切换单个工具不改写段文本，仅 schema 变化处失效。

### Tool schemas

#### What the model sees

[docs/tool-catalog.md](../../../docs/tool-catalog.md#deepseek-aidsh-tool-browser) 中生成的 `browser_*` schema。超时预算是部署设置，不是模型参数。

#### Token effect

全开时五个 schema；每个关闭的开关恰好移除自己的 schema。

#### KV Cache effect

定义不变时前缀稳定。

### Results

#### What the model sees

页面状态类工具渲染 `Navigated to <url>` 风格的头部加可选标题与内容，按 `maxOutputChars` 截断并附截断提示；失败呈现为 `Error: <message>`。截图渲染命名所存 PNG 的 `<path>` 信封。

#### Token effect

数据相关的结果在压缩前随历史重发。

#### KV Cache effect

仅追加。

## Known Limitations and Deferred Work

- **截图返回文件路径而非内联像素** —— 基于附件的图像块（即 `read_image` 机制）是既定升级路径；当前具备图像输入的模型可对返回路径调用 `read_image` 组合完成。
- **仅 CSS 选择器交互** —— 基于元素引用的操作等待 seam 的快照词汇。
- **每个上下文一条串行会话** —— 并发调用串行执行；并行标签页工作需要先设计会话多路复用。
- **无 web 专属权限策略** —— 与 web 工具一样，动作不经过 `ctx.approval` 执行；需要确认的部署添加 `tools/pre-execute` 策略。
