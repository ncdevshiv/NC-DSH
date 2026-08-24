# @deepseek-ai/dsh-browser

[English](README.md) | 中文

浏览器自动化能力 seam（`ctx.browser`）的 Service Definition：提供方注册表与按提供方选择的会话启动，以及共享的请求/结果词汇和 `BrowserError` 分类。选择语义与 [`dsh-web`](../../web/web/README.md) 一致：已配置的提供方优先；未配置时恰好一个可用提供方自动入选，因此选择从不依赖注册顺序。

- [`browser-moli/`](../browser-moli/README.md) 驱动本地 [moli](https://github.com/lexmount/moli) 无头浏览器并注册到本 seam。
- [`tool-browser/`](../tool-browser/README.md) 拥有稳定的模型侧 `browser_*` 工具。

## Model Experience

间接地，通过 `dsh-tool-browser`：它拥有全部 schema、提示词段与结果渲染，provider 失败以结构化 `BrowserError` 呈现。

#### KV Cache effect

无直接失效；由具名消费者拥有任何请求前缀变化。

## Known Limitations and Deferred Work

- 在提供方给出页面状态之前，seam 仅暴露 CSS 选择器交互；基于元素引用的操作与多标签会话复用是消费方声明的延后工作。
- 截图交付向消费者返回 PNG 字节；模型可见的内联像素等待 `dsh-tool-browser` 中基于附件的图像块。

子系统参考——会话/页面请求与结果、可用性、`BrowserError`——见本包的 [src/types.ts](src/types.ts)；[web 能力决策](../../../.agents/notes/implemented/architecture/2026-06-24-web-capability-seam.md)记录了两个注册表共同遵循的单一 seam 先例。
