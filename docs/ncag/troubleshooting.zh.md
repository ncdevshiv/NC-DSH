---
description: "NC-AG 运维实际会遇到的失败、各自含义与确切修复。"
kind: "guide"
---

# NC-AG 故障排查

[English](troubleshooting.md) | 中文

## 摘要

本页把 NC-AG 报告的每个失败映射到原因与修复。平台会大声失败并指出找不到的内容，因此多数条目是缺失的配置值，而不是缺陷。

## 目录

- [启动失败](#boot-failures)
- [任务执行失败](#task-execution-failures)
- [验证失败](#verification-failures)
- [集成失败](#integration-failures)
- [成本与预算](#cost-and-budget)
- [控制平面](#control-plane)

-----

<a id="boot-failures"></a>
## 启动失败

| 消息 | 原因 | 修复 |
|---|---|---|
| `ncag: NCAG_DATABASE_PATH must name the platform database file` | 变量未设或为空 | 导出绝对路径 |
| `ncag: NCAG_WORKSPACE_ROOT must name the directory that holds agent workspaces` | 同上，工作区根目录 | 导出绝对目录 |
| `ncag: NCAG_MODEL_PRICING must hold the model price table so cost accounting cannot report zero` | 只有独立 `ncag` profile 要求它 | 导出 JSON 数组，或使用随包 Web preset 并在 NC-AG 设置页设置价格 |
| `ncag: the model price table must be a non-empty JSON array` | 独立 profile 中格式错误或为空 | 每个条目需要 `provider`、`model` 与两个非负的每百万价格 |
| `cannot get property "llm" without inject` | 服务读取了它未注入的服务 | 服务以严格方式读取可选服务；缺失的服务会成为已报告的路由缺失，而不是启动崩溃 |
| `dsh: MISSING_CREDENTIAL` | harness LLM 插件没有该提供方的凭据 | 设置 `DEEPSEEK_API_KEY` 或通过 Web 模型页存储 |
| `failed to import ... lib/index.js` | 包未构建或未链接 | 运行产物构建，再重新安装工作区链接 |

<a id="task-execution-failures"></a>
## 任务执行失败

| 任务原因 | 原因 | 修复 |
|---|---|---|
| `no model route is configured, so no agent can execute this task` | 平台没有模型端口 | 挂载 harness LLM 服务，或向 `runPlatform` 提供端口 |
| `ncag: route <provider>/<model> has no configured price` | 路由找到可用路由但部署未定价 | 添加价格条目 |
| `ncag: no registered profile serves capability ...` | 任务所需能力没有档案提供 | 在提交请求前注册具备该能力的档案 |
| 工具结果中的 `refused by policy (...)` | 档案缺少工具或权限，或命令匹配破坏性模式 | 有意放宽档案，或改变所请求的方法 |
| `ncag: task ... cannot start: dependencies still open: ...` | 调度器被要求启动被阻塞的任务 | 这是守卫在正常工作；依赖完成后任务会启动 |
| `recovery exhausted N retries` | 任务失败达 `maxRetries` 次 | 检查发现与证据，修复根本原因，再重新提交 |

<a id="verification-failures"></a>
## 验证失败

| 证据或原因 | 原因 | 修复 |
|---|---|---|
| `criterion ... declares no command` | 该方法在此部署中没有命令 | 把方法加入项目 `checks` |
| `spawnSync <exe> ENOENT` | 配置的 shell 可执行文件不存在 | 把 `NCAG_SHELL` 设为存在的 shell |
| 退出码非零且输出真实 | 检查确实失败 | 修复交付行为并重新提交 |
| `acceptance criteria without passing evidence: ...` | 某标准没有通过证据行 | 修复检查后重跑验证 |

<a id="integration-failures"></a>
## 集成失败

| 报告 | 原因 | 修复 |
|---|---|---|
| `textual conflict: ...` | 两个变更集改动了相同行 | 在新提交中解决；基线分支不变 |
| `integration validation failed: ...` 且合并已回滚 | 整合后的树未通过验证 | 修复变更集；基线分支不变 |
| `base branch moved since submission` | 智能体工作期间项目已推进 | 针对当前基线重新提交 |
| `ncag: git ... failed in ...` | git 缺失或仓库不是工作树 | 设置 `GIT_EXECUTABLE`，或传入存在的仓库路径 |

<a id="cost-and-budget"></a>
## 成本与预算

达到 `maxCostUsd` 的项目在跨过上限的那次调用记录后即变为 `PAUSED`，并产生一个 `user-reserved` 决策请求追加支出。通过 `GET /api/projects/:id/decisions` 查看，用 `POST /api/decisions/:id/resolve` 解决，再 `POST /api/projects/:id/resume`。

<a id="control-plane"></a>
## 控制平面

| 现象 | 原因 | 修复 |
|---|---|---|
| `401 {"error":"invalid token"}` | bearer 令牌错误或缺失 | 使用配置的 `NCAG_TOKEN`，作为首部或 `?token=` |
| `500 {"error": ...}` | 操作抛出异常 | 消息就是编排器自身的原因；读取它、修复原因并重试 |
| 面板显示 `audit chain broken at N` | 记录被修改或删除 | 调查；链是记录历史的证明 |
| 事件流反复重连 | 服务器重启或代理关闭了流 | `EventSource` 会自动重连；面板在每个事件上重新渲染 |
