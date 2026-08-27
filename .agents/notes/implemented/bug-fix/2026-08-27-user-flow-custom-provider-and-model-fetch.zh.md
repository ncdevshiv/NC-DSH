# Agent Note: 修复自定义提供商与模型拉取的用户流程

Status: implemented

[English](2026-08-27-user-flow-custom-provider-and-model-fetch.md) | 中文

## 问题

对每条用户流程的端到端追踪——自定义提供商创建、模型拉取、工作区、分组合并及周边设置界面——发现四处影响用户可见行为的缺陷，已在本 PR 一并修复：

1. **自定义提供商的图像开关从未持久化。** `ModelListEditor` 读写 `model['input']`，而 `DeepSeekModelsEditor`、`llm-ai-sdk` 的 `catalogModel` 校验与 `AiSdkAdapter` 均读取 `inputModalities`。用户在自定义提供商上勾选图像后，磁盘上写入 `input: ['text','image']`，适配器则完全忽略，模型始终视为仅文本。通过拉取采纳的候选同样写入 `input`。

2. **自定义提供商创建接受带空白的取值。** `CustomProviderCard` 未经修剪即校验 `route` 与 `baseURL`，`route = " acme "` 之类被 `ROUTE_PATTERN` 拒绝，但 `baseURL = "   "` 仍通过 `ready` 检查。配置以原始字符串入库，侧车（sidecar）随即以不透明的 `CONFIG`/`network` 错误失败。

3. **Sidecar 的 `childExitOrTimeout` 在每次请求上泄漏计时器与监听器。** 每个 `request()` 均注册 `child.once('exit'|'error')` 与 120 秒 `setTimeout`，成功路径上从未清理。突发拉取或流式请求会无界累积处理器并持有计时器至上限。

4. **未分组的当前空白会话消失。** `tree.ts:groupByWorkspace` 组装 `stray`（未分组）集合时额外执行 `&& !s.blank`，导致 `current` 为空白且未分组的会话即便 `sessionVisible` 已允许当前空白也被丢弃。归属于工作区的空白会话可见，未分组的不可见，破坏 `connectChat` 复用体验。

关联笔记：[whole-section provider reset framing](2026-08-23-whole-section-provider-reset-framing.md)、[workspace UI product flow](../feature/2026-07-25-workspace-ui-product-flow.md)、[moli session/provider boundary fixes](2026-08-24-moli-session-provider-boundary-fixes.md)。

## 决策

- **视觉字段统一为 `inputModalities`。** `ModelListEditor.declaresVision` 优先检查 `inputModalities`，以 `input` 作为历史兼容回退；`setVision` 写入 `inputModalities` 并丢弃历史 `input`；`adopt` 写入 `inputModalities`。界面到 `resolveAdapterOptions` → `AiSdkAdapter` → `catalogModel` 校验的链路现已一致。`provider-form.client.spec.tsx` 与 `components.client.spec.tsx` 中相关用例已由 `input` 改为 `inputModalities`。
- **在校验与落盘边界执行修剪。** `CustomProviderCard` 引入 `trimmedRoute`/`trimmedBaseURL` 用于 `routeInvalid`、`routeTaken`、`ready`、提示文案、`deriveKeyRef`、`displayName`、`baseURL` 以及 `providers.<route>` 设置路径。原始输入仍在界面上原样呈现，直至被判定。
- **`CustomProviderCard` 在版本号竞争上将 `settings-conflict` 呈现为本地化 `conflict` 文案**，而非原始主机消息。
- **`AiSidecarClient.request` 接受可选 `AbortSignal`，`discoverModels` 将其透传。** `childExitOrTimeout` 单次捕获 `child`，安装具名 `onExit`/`onError`，在任意退出/错误时清理计时器，并在超时时移除处理器。`model.discover` 路径现将调用方的 signal 经 `index.ts:registerModelDiscovery` → `sidecar.discoverModels` 透传。侧车边界上不再透传缺失非空 `id` 的畸形拉取行。
- **`groupByWorkspace` 不再对 `stray` 过滤空白。** 当前空白会话现可在未分组桶中可见，与归属工作区的空白会话行为对称。`tree.client.spec.ts` 新增覆盖该用例。

## 已考虑的替代方案

**在 `resolveAdapterOptions` 中迁移已存的 `input` 取值。** 已拒绝：磁盘上的 `settings.yaml` 为用户可编辑的 YAML；在解析阶段静默改写会在无设置写入的情况下改变用户拥有的文件，且下一次图像开关切换即可修复该瞬态不一致。未来涉及该层的设置迁移可一并规范化残留的 `input` 字段。

**将 sidecar 监听器修复推迟至原生侧分页/容量重构。** 已拒绝：该泄漏独立且修复成本低；`openai_compat.rs` 中的分页与硬编码 `ModelInfo(128_000, 8192)` 仍作为具有明确 Rust 范围的后续事项。

## 后果

- 自定义提供商的图像选择可持久化，流式网关与同一字段保持一致。含 `input` 的历史配置在编辑前仍可正确渲染。
- 带空白填充的路由与端点在表单侧即被拒绝，不会以带空格的形式落入 `settings.yaml`。
- Sidecar 超时/退出处理器不再累积；已中止的拉取请求以 `SidecarProtocolError(kind='cancelled')` 及时结束。
- 未分组的空白当前会话现出现在浏览器中，与归属工作区的空白会话一致。
- 验证：`bun run typecheck` 无错误；`bun run test --run packages/client/ui-settings-models packages/client/ui-workspace packages/llm/llm-ai-sdk` 全部通过（383 + 155 + 228 用例）；`tree.client.spec.ts` 新增 `shows the current ungrouped blank session in the Ungrouped bucket`。

## 验证

- `bun run typecheck` —— 零错误
- `bun run test --run packages/client/ui-settings-models` —— 10 文件、228 用例通过
- `bun run test --run packages/llm/llm-ai-sdk packages/client/ui-workspace` —— 11 文件、155 用例通过
- 新增用例：`tree.client.spec.ts:shows the current ungrouped blank session in the Ungrouped bucket`
