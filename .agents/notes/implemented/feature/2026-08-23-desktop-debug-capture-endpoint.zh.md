# Agent Note: 桌面调试窗口截图端点

Status: implemented

[English](2026-08-23-desktop-debug-capture-endpoint.md) | 中文

## Problem

调试桌面 shell 渲染出的 UI 只能依赖操作系统级截屏：外部工具抓取的是窗口矩形上重叠的所有像素，窗口被遮挡或最小化时得到的就是错误的画面，而且整条流程对 dsh agent 和其他本地工具完全不可达，必须有人在旁操作。UI 缺陷只有在用户恰好截到正确瞬间时才会暴露。

## Decision

Electron 主进程在窗口之外额外提供一个回环调试截图监听（`apps/desktop/main.mjs`）：`GET /debug/windows.json` 列出每个窗口的 id、标题、URL、最小化状态与边界；`GET /debug/screenshot.png[?window=<id>]` 通过 `webContents.capturePage()` 返回该窗口页面内容的 PNG 字节，不受屏幕遮挡影响。最小化的窗口会在截图前被还原、截图后重新最小化；从未绘制过的窗口返回 409。

发现机制不进日志：每个实例把 `{ pid, port, token }` 写入 `%TEMP%/dsh-desktop-debug/endpoint-<pid>.json`，退出时删除，stdout 只打印文件位置。两条路由都要求每次运行随机生成的 `token`；仅绑定 127.0.0.1，响应不带 CORS 头，因此持有该发现记录即持有截图权限。`DSH_DESKTOP_DEBUG_CAPTURE=0` 可整体禁用监听。`scripts/desktop-screenshot.mjs` 是自带的外部客户端（按 pid 存活性做发现、列窗口、单窗截图），也是 dsh agent 经由其 shell 工具调用的路径。

## Alternatives considered

- **第二实例 CLI 转发**（`app.requestSingleInstanceLock` + argv）：全部留在 Electron 内，但每次截图都要付出完整进程启动的代价，多开发实例还会让锁归属复杂化。
- **投放文件触发**（watcher 收到请求后写 PNG）：不开端口，但没有结构化窗口列表、存在轮询延迟，还多出一份要持续维护的文件契约。
- **经 `dsh web` 宿主平面暴露截图**：后端不拥有任何窗口；截图必须位于拥有被截表面的进程里。
- **操作系统级截图（PrintWindow / CopyFromScreen）**：需要前台权限、会产生重叠像素伪影、也没有程序化发现——正是本次要修复的失败点。

## Consequences

任何能读取用户临时目录的本地进程都可以静默截取所有打开的 shell 窗口；token 把范围限制在记录持有者之内，但无法阻止同用户进程——这本就是 Electron 已在运行的信任域。最小化窗口在截图时会可见地闪动一下，因为图标化状态下 Chromium 没有合成器表面。崩溃残留的记录由客户端按 pid 存活性跳过，而非主动清理。该端点仅供调试：它呈现的状态，回环服务器加操作系统本来就已暴露给同一用户，且不承载任何留在 `dsh web` 之后的业务逻辑。
