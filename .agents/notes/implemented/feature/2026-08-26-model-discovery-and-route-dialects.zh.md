# Agent Note: 单适配器上的模型发现与路由协议

Status: implemented

[English](2026-08-26-model-discovery-and-route-dialects.md) | 中文

> 完成[单适配器迁移](../architecture/2026-08-25-single-llm-adapter-via-ai-sdk.md)的遗留项：自定义提供方入口与端点探询正式落地，sidecar 客户端的进程生命周期彻底闭合。

## Problem

迁移删除了每路由的 `api` 协议字段，而它正是 Models 页面用于开放「添加自定义提供方」的 schema 事实——于是该入口永远禁用；同时 sidecar 也不存在任何方法去探询一个配置尚未存储的端点。另一方面，`AiSidecarClient` 把子进程 spawn 进局部变量却从不保存：`dispose()` 杀不掉任何东西，一次失败的初始化会在清掉防重入 memo 的同时留下一个仍然健康的进程，每次测试运行或插件重载都泄漏活着的 node 进程——用户机器上累积的数百个孤儿 `node.exe` 正源于此。

## Decision

provider profile 接受可选的 `api` 协议声明——`openai-compatible`、`anthropic`、`google` 三者之一。省略时按路由 id 推导，推导规则与 sidecar 一贯的行为完全一致，既有组合不受影响；显式声明让自定义 URL 可以讲原生格式。推导逻辑只存在一份（`defaultApiOf`），精确镜像 SDK 的按 id 选择。

sidecar 协议新增 `model.discover`：给定 `{api_key?, base_url?, api?}`，它为这一次列表调用构建临时 provider，绝不触碰已配置世代，因此探询草稿不会惊扰在途流。configure 路径以路由名注册适配器（`AiClientBuilder.provider_as`），因为两条路由可共享同一传输格式，而引用按 `route:model` 解析；未知协议在任何网络调用之前即以类型化配置错误响亮失败。发布方需重新构建 release 二进制以获得该协议面。

适配器注册 `ctx.llm.registerModelDiscovery('llm-ai-sdk', …)`：仅指名既有路由的草稿由该路由的参考 catalog 直接作答，不发起 sidecar 调用；其余情况按「草稿覆盖 → 路由 profile → 路由 id 表」解析协议，草稿未带密钥时回退到该路由已存凭据，OpenAI 兼容探询必须有 base URL，在边界处把 wire 行映射为 `LlmDiscoveredModel`，一次性密钥事后不留任何副本。

`AiSidecarClient` 在任何 await 之前保留子进程引用；拆除收敛为一条幂等且按世代感知的路径，由退出、spawn 失败、初始化失败与销毁共用——垂死子进程的退出事件无法拆掉其后继者的传输。新增只读 `pid` 访问器，为测试与诊断提供生命周期观察缝。

Models 页面上，`protocolChoices()` 从恢复的 schema 字段读取协议并集，创建卡片恰在命名空间挂载时启用；手声明路由通过与创建卡相同的字段编辑其显示名与协议；所有路由的模型列表统一走支持探询的列表编辑器。settings 附着时通过注册句柄的原子 `replace` 重导目录——裸重复注册会与已声明的 id 冲突并把整条 settings 接线在附着中途拆毁，这正是热更新此前从未生效的原因。

## Alternatives considered

**保持入口禁用，把 `settings.yaml` 作为声明路径写进文档。** 这是迁移当时记录的选择，此处推翻：该页面就是添加提供方的产品入口，永久禁用的主操作不是「已知限制」而是缺失的功能。

**在浏览器里按端点 URL 启发式推断协议。** 落败：它把 SDK 的选择表用另一门语言复制一遍并必然漂移；显式字段直达 sidecar，真相只有一个持有者。

**只在 `dispose()` 里补 kill。** 两头都不够：按失败初始化重生的循环需要初始化失败路径上的拆除；而没有世代感知，杀掉已被替换的子进程会顺带拆掉其继任者的传输。

## Consequences

- 部署必须重建或更换 `ai-sidecar` 以获得 `model.discover`、`configure` 的协议透传与按路由名注册；旧二进制对发现请求响亮失败而非静默降级。
- 每一代 llm-ai-sdk 客户端现在都会终结其子进程：失败的初始化、销毁与退出各自留下零个存活进程，watch 模式测试与插件重载不再累积孤儿。
- 发现请求的取消沿用传输层既有上限而非请求信号：探询在 120 秒 JSON-RPC 上限处结算，与其余 sidecar 调用一致。
- 真实云端 e2e 依旧按密钥门控；本地覆盖以真实 release 二进制加脚本化 OpenAI 兼容 HTTP 服务跑通全栈，包括一次真实的列表往返。

## Testing

`bun run vitest run packages/llm/llm-ai-sdk packages/client/ui-settings-models packages/settings packages/llm/llm` 全绿（683 通过；一个已提交的 Windows 符号链接测试在无符号链接权限的机器上失败，与本变更无关）。`DSH_AI_SDK_SIDECAR=<release binary> bun run vitest run --config vitest.e2e.config.ts packages/llm/llm-ai-sdk` 两个真实二进制用例全部通过：流式补全与未存端点的发现（含存量密钥认证）。`bun run typecheck` 全仓零错误。
