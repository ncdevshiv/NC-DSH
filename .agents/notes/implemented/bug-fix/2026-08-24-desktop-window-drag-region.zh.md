# Agent Note: Desktop window caption-band drag region

Status: implemented

[English](2026-08-24-desktop-window-drag-region.md) | 中文

## Problem

Electron 桌面外壳（`apps/desktop`）以无边框窗口（`titleBarStyle: 'hidden'` 加 `titleBarOverlay`）渲染 Web GUI。除三个原生标题按钮之外，这类窗口自身没有任何可拖动区域：只有页面用 `-webkit-app-region: drag` 标记的元素才能承担拖动。客户端从未声明过这样的区域，因此外壳改用隐藏标题栏之后，窗口完全无法移动。缩放边框与标题按钮仍然有效，这让缺陷看起来像局部的显示问题，而非一个缺失的契约。

## Decision

标题栏带由 Web 客户端负责。`apps/web/index.html` 渲染一个默认惰性的条带元素（`#dsh-titlebar-drag`，默认 `display: none`）。`apps/web/src/main.ts` 在存在外壳 preload 桥（`window.dshDesktop`）时设置 `html[data-dsh-desktop]`——与 `ThemePresenter` 和 boot 主题脚本使用的是同一个特性探测。`packages/client/web/src/base.css` 将真正的规则挂在该属性上：条带变为按 Window Controls Overlay 环境变量（`env(titlebar-area-x/y/width/height)`）定宽定高的固定拖动柄；同时为布局预留该条带（`body` 切换为 `border-box` 并加 `padding-top: env(titlebar-area-height, 32px)`，`#root` 的视口 min-height 以同一数值重新取基，避免把内容推回条带之下）。

该行为在 OS 层面得到固定：活动窗口上的 `WM_NCHITTEST` 在整条带上返回 `HTCAPTION`（原生按钮区除外），条带之下返回 `HTCLIENT`；合成的鼠标拖动能移动窗口。浏览器不会设置该属性，渲染与从前完全一致。

## Alternatives considered

- **以 `@media (display-mode: window-controls-overlay)` 作为门控**（已安装 PWA 的惯例做法）：实测 Electron 即使 overlay 生效也不匹配该 media feature，而 `env(titlebar-area-*)` 能正确解析——条带会在唯一需要它的场景里保持失效。manifest 也使用 `display: fullscreen` 且未声明 `display_override`，当前不存在已安装 PWA 这一消费方。
- **不挪动内容、让条带直接覆盖在内容之上**：顶带内有可交互界面（侧栏品牌区、会话头部控件），预留条带才能保住所有控件的点击。
- **回退到操作系统标题栏**：等于放弃外壳有意引入的主题化 overlay 边框。
- **从 `navigator.windowControlsOverlay.getTitlebarAreaRect()` 读取几何信息**：冗余——CSS `env()` 已承载几何信息；一次性的属性门控是唯一的 JavaScript。

## Consequences

在桌面外壳内，整个顶带专用于拖动：拖动即移动窗口，双击切换最大化，应用内容整体下移一个条带的高度。原生标题按钮对该区域的命中测试优先于页面区域，因此即使条带横跨按钮区，最小化/最大化/关闭仍然可用。若未来出现已安装 PWA 的 WCO 场景，需要在属性门控旁补充标准的 media-query 分支；目前不存在此类消费方。
