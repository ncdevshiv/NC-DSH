# Agent Note: moli 会话拆除与提供方边界修复

[English](2026-08-24-moli-session-provider-boundary-fixes.md) | 中文

Status: implemented

## 问题

moli CDP 会话在普通路径上报告错误结果：页面在 `Page.navigate` 尚未结算时便触发 load 事件，会让导航空耗整个预算；`close()` 会等待阻塞中事件等待者的期限并泄漏其计时器；页内执行异常被折叠为空结果，使 click/type 在毫无效果时报成功；串行链在会话生命周期内为每次操作保留一个已结算的 promise；并且导航把 `file:`/`javascript:` 等非 http(s) 目标直接送进协议，而 seam 契约把协议拒绝指派给 provider。相邻的提供方边界存在同类问题：HTTP 基本认证无法编码的 SearXNG 凭据对表现为逐次搜索的 `WEB_PROVIDER_ERROR`，而不是加载时的配置错误；moli fetch 提供方先按抛出值形态分类被杀进程——背板超时可能被报成 `WEB_ABORTED`——并且在调用方已经中止后仍会启动子进程。打包的 Python 运行时携带了浏览器 seam 与 provider 却缺少消费方工具，导致该发行版无法挂载任何调用 `browser_*` 的组合。

## 决策

moli CDP 连接在关闭时以 `BROWSER_SESSION_CLOSED` 拒绝所有未决事件等待者与未决命令；关闭后的发送与事件等待同样立即拒绝，不再泄漏裸传输异常；目标发现会在类型转换前校验 HTTP 状态码与响应体形状。会话在发送 `Page.navigate` 之前注册 load 事件等待者；检查 `Runtime.evaluate` 响应中的 `exceptionDetails` 并抛出携带页内描述的 `BROWSER_EVALUATION_FAILED`；改为只保留最新操作的滚动单 promise 串行链；并在任何 CDP 流量承载之前把导航目标校验为绝对 http(s) URL（否则报 `BROWSER_INVALID_URL`）。SearXNG 在插件加载时拒绝不可按 Latin-1 编码的凭据对。moli fetch 提供方先看调用方信号是否已中止，再看背板是否耗尽（`WEB_FETCH_TIMEOUT`），最后才按中止形态的抛出值分类，并在启动前拒绝已触发的中止信号。打包运行时清单加入 `@deepseek-ai/dsh-tool-browser`，与 `dsh-browser`/`dsh-browser-moli` 一道补全其分发的浏览器闭包。

## 测试

moli 套件为每项修复钉住行为：命令执行期间的 load 事件照常解析、有操作阻塞在 CDP 事件上时 close 立即完成、click 与 snapshot 呈现页内执行失败、scheme 拒绝且零帧发出。一个经真实 Loader 启动的测试专用 cordis.yml 同时挂载 seam、provider 与消费方，断言注册的五工具表面与 guidance 段落，以及二进制缺失时的结构化 no-usable-provider 失败。SearXNG 覆盖加载期凭据拒绝；fetch 提供方覆盖分类优先级与中止先行不启动守卫。

## 已考虑的替代方案

### 为什么不等待更丰富的导航生命周期事件？

等待 frame 或生命周期事件组合更贴近 Chrome 的模型，但会把刻意最小化的 CDP 客户端耦合到更多协议表面。等待者先于发送的注册顺序无需新增协议依赖即可消除丢事件竞态。

### 为什么不在消费方 schema 层校验导航 URL？

schema 模式会把契约复制到远离其归属 provider 的地方，而直连 `ctx.browser` 的调用方根本不经过工具 schema；seam 的请求类型已写明"其他 scheme 由 provider 拒绝"，provider 就是强制执行点。

### 为什么不从 `python/sdk-runtime` 移除浏览器依赖？

该清单的依赖闭包就是分发插件集本身，且每个兄弟能力都在那里附带自己的消费方。移除会让打包运行时无法挂载使用 `browser_*` 的组合；加入 `@deepseek-ai/dsh-tool-browser` 才与 `dsh-tool-web` 对称地补齐预期闭包。

### 为什么不继续仅按抛出形态分类 fetch 失败？

纯形态分类之所以有效，只是因为当前 runner 恰好抛出普通 Error；任何把进程被杀呈现为 DOMException 的传输层都会把超时误报为取消。信号状态优先的排序依赖所有权事实（本提供方创建的两个信号），而非 runner 内部实现。

## 后果

快速页面即时解析导航而不再超时；拆除在连接结算范围内完成而不受等待者期限约束；交互不再静默假成功，代价是新增一个实现自有代码（`BROWSER_EVALUATION_FAILED`），由开放式代码契约下的容忍未知规则覆盖。凭据配置错误只失败一次、在加载时、带明确修复指引。暂缓加固保持开放并有文档：贯穿会话操作的端到端 `AbortSignal` 传播、防孤儿 serve 进程的宿主退出收尾、可配置的 evaluate/截图期限与交互后 settle 延迟、截图临时文件清理，以及跨 seam JSDoc、两个 moli provider 与子系统页面的 `available()` 探测措辞统一。
