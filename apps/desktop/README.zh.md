# @deepseek-ai/dsh-desktop

[English](README.md) | 中文

Web profile 之上的 Electron 桌面壳。主进程渲染 `dsh web` 已提供的回环服务器——不移动任何业务逻辑：会话、工具与客户端插件模块表都留在宿主侧，窗口只是原生 chrome 加一个加固（`contextIsolation`、无 node 集成）的渲染进程。

## 运行

```sh
bun run desktop        # built frontend: spawns `dsh web --no-open` + the Electron window
bun run dev:desktop    # dev mode: also runs scripts/dev-web.ts so source edits rebuild
                        # lib/client.js and apps/web/dist; the host stat-polls those
                        # artifacts and pushes `rebuilt` frames — the window hot-reloads
                        # changed client plugins (HMR) without restarting Electron
```

两个命令在 Ctrl+C 时都会拆掉整棵子进程树。`DSH_DESKTOP_URL` 覆盖窗口加载的 URL；启动器把它设为所派生宿主的端口。

## 调试窗口捕获

运行期间，壳会服务一个回环 HTTP 端点，任何本地进程——编辑器扩展、测试框架，或 dsh 代理本身经其 shell 或 web 工具——都能借此截图它的任意窗口用于调试，包括屏幕上被遮挡的窗口。

两条路由，都需要每次运行生成的 `token` 查询参数：

- `GET /debug/windows.json` —— 每个窗口的 id、标题、URL、最小化状态与边界。
- `GET /debug/screenshot.png[?window=<id>]` —— 该窗口页面内容的 PNG；不带 `window` 时取聚焦窗口（否则第一个）。最小化窗口会为捕获而恢复并再次最小化。

外部进程不解析日志：每个实例把 `{ pid, port, token }` 发布到 `%TEMP%/dsh-desktop-debug/endpoint-<pid>.json`（退出时移除），日志只打印该文件位置。随附客户端封装了发现与捕获：

```sh
node scripts/desktop-screenshot.mjs --list          # live shells and their windows
node scripts/desktop-screenshot.mjs shot.png        # focused window of the (single) shell
node scripts/desktop-screenshot.mjs shot.png --pid 1234 --window 1
```

信任模型：只绑定 127.0.0.1，响应不带 CORS 头，token 每次运行随机——持有发现记录即获得捕获权限，因此请把该文件当作凭据对待；截图可能包含窗口渲染的任何内容。设置 `DSH_DESKTOP_DEBUG_CAPTURE=0` 可整体禁用该监听。

## 已知限制与顺延工作

- 尚无打包方案（electron-builder / 单 exe）：当前以面向开发的 workspace 应用形态发布。
- 窗口信任 `DSH_DESKTOP_URL` 指向的任意地址；启动器只会把它指向 127.0.0.1。
