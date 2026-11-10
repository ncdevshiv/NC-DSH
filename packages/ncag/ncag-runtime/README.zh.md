---
description: "NC-AG 智能体运行时：与提供方无关的模型端口、受限工作区工具、提示词装配，以及智能体轮次循环。"
kind: "package-library"
---

# @deepseek-ai/dsh-ncag-runtime

[English](README.md) | 中文

## 摘要

本包执行单个 NC-AG 智能体。它定义模型端口与工具契约，提供六个工作区工具与两个协调工具，装配系统提示词与任务消息，把 harness LLM 服务适配到该端口，并运行轮次循环：询问模型、执行并等待其请求的工具、把结果回填、在最终回答或步数预算处停止。

## 目录

- [端口](#ports)
- [工具](#tools)
- [提示词装配](#prompt-assembly)
- [循环](#the-loop)
- [harness 模型端口](#the-harness-model-port)
- [已知限制与后续工作](#known-limitations-and-deferred-work)
- [开发者说明](#dev-note)

-----

<a id="ports"></a>
## 端口

`LlmPort.complete(request)` 返回文本、工具调用与用量。`LlmRequest` 携带提供方、模型、可选推理强度、系统提示词、消息、工具模式、输出上限与取消信号。`ToolEnvironment` 携带工作区、智能体、任务、项目、信号，以及协调工具用于联系编排器的可选 `AgentCoordination` 接口。

端口实现是生产与测试之间的唯一差异：随包提供的适配器经由 harness LLM 服务路由，测试提供脚本化实现。其余一切（循环、工具、提示词）运行完全相同。

<a id="tools"></a>
## 工具

| 工具 | 权限 | 行为 |
|---|---|---|
| `read` | workspace:read | 带行号的行文本，超过 200 KB 截断 |
| `write` | workspace:write | 创建父目录并写入完整内容 |
| `edit` | workspace:write | 恰好替换一处，零处或多处均拒绝 |
| `glob` | workspace:read | 遍历工作区，支持 `**` 模式，最多 200 条 |
| `grep` | workspace:read | 正则搜索并给出文件与行号，最多 200 条匹配 |
| `shell` | workspace:shell | 在工作区执行命令，返回退出码与输出 |
| `request_help` | communication:send | 向编排器请求专家，专家的真实答复作为工具结果返回 |
| `hand_off` | communication:send | 发送交接包，并由编排器把任务重新排队给另一个智能体 |

协调工具通过 Promise 作答，因为它们要等待编排器；没有挂载协调接口的工具会准确报告这一点，而不会假装请求已被处理。

每个路径都被解析到工作区内部，越界即拒绝。`resolveShell` 依次取 `NCAG_SHELL`、Windows 上的 `powershell.exe` 与其他平台的 `pwsh`，因此无法解析的 shell 会让调用失败并指出所查找的可执行文件，而不是静默跳过工作。

<a id="prompt-assembly"></a>
## 提示词装配

`buildSystemPrompt(profile, context)` 渲染身份、工作风格、专长、职责、限制、质量标准与升级规则，随后是项目名称与指令、已批准的架构、契约与约束。`buildAssignmentMessage(task, context)` 渲染任务、描述、输入、预期产出、验收标准、已完成依赖的产出，以及发给该智能体的协调消息。

<a id="the-loop"></a>
## 循环

`runAgent(port, request)` 每步构建一个 `LlmRequest`，用确切的请求调用 `beforeRequest`，在 `onToolCall`/`onToolResult` 下执行每个请求的工具，把结果作为工具消息追加到对话，并返回 `{ finalText, completed, steps, usage, transcript }`。参数载荷格式错误、工具名未知与缺少必需参数都会变成失败的工具结果而非异常，因此模型可以纠正。耗尽 `maxSteps` 时返回 `completed: false` 与目前的对话。

<a id="the-harness-model-port"></a>
## harness 模型端口

`createLlmPort(llm)` 把运行时请求映射为 `GenerateOptions`，流式读取 harness 分片，按块索引累计文本增量与工具调用增量，返回装配好的块。工具调用参数是模型产生的原始 JSON，循环随后解析并校验它。

<a id="known-limitations-and-deferred-work"></a>
## 已知限制与后续工作

- **每块一个工具调用，无图像。** 端口装配文本与工具调用；工具结果中的图像与文件块尚未投影，且目前没有 NC-AG 工具会产生它们。
- **工具执行是同步的。** 长命令会阻塞智能体的一步。信号可限制该次运行，按调用期限属于工具契约，待部署需要时加入。

<a id="dev-note"></a>
## 开发者说明

<details>
<summary>维护者工作背景 —— 点击展开</summary>

运行时有意依赖端口而非服务：它不导入任何 Cordis 服务，也不导入任何文件系统后端，这使编排器可以在 harness 进程内、测试中或独立运维工具中驱动它。

</details>
