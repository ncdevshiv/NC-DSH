# browser/ — 浏览器自动化能力族

[English](README.md) | 中文

本族提供提供方中立的浏览器自动化——启动无头浏览器会话、导航、读取页面状态、按 CSS 选择器交互并捕获截图——以及消费它的模型工具。

| 包 | 角色 | ctx 键 |
|---|---|---|
| [`browser/`](browser/README.md) | 定义浏览器提供方注册、选择与共享错误 | `ctx.browser` |
| [`browser-moli/`](browser-moli/README.md) | 驱动本地 [moli](https://github.com/lexmount/moli) 无头浏览器（`serve` + CDP），每个会话一个隔离进程 | 注册到 `ctx.browser` |
| [`tool-browser/`](tool-browser/README.md) | 向模型暴露浏览器导航、检查、交互与截图 | 注册到 `ctx.tools` |

选择语义与 [web 族](../web/README.md)一致：提供方注册到同一个 seam；启动时已配置的提供方优先，未配置时恰好一个可用提供方自动入选。在外部二进制可解析（`$MOLI_BINARY` 或 `PATH`）之前提供方保持休眠，因此挂载它们在部署未选择加入时不产生任何开销。

模型侧表面是可选的：在 profile overlay 中挂载 `@deepseek-ai/dsh-tool-browser` 即可加入 `browser_*` 工具。
