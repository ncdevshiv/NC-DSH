---
description: "NC-AG 平台的浏览器监控与操作 Remote 命名空间：快照、时间线、决策，以及面向宿主平面编排器的项目/智能体控制。"
kind: "package-reference"
---

# @deepseek-ai/dsh-api-ncag-controller

[English](README.md) | 中文

## 摘要

拥有设置页 NC-AG 页面读取的 `ncag` Remote 命名空间。每个方法都委托给宿主平面 `ctx.ncag` 服务持有的编排器，因此本包只添加浏览器安全的线上投影与错误翻译，不含平台行为。Web 包按宿主平面挂载它，使页面无需某个会话选中 NC-AG 模式即可监控平台；由于 `ncag-service` 按数据库路径去重，preset 自身的平台挂载与它共享同一个编排器。

## 目录

- [使用本包](#use-this-package)
- [理解实现](#understand-the-implementation)
- [已知限制与后续工作](#known-limitations-and-deferred-work)

-----

<a id="use-this-package"></a>
## 使用本包

把本行挂在 Web 应用旁，位于 `ncag-settings` 与 `ncag` 行之后。`@deepseek-ai/dsh-api-remotes` 挂载生成的贡献后，浏览器即可读取 `ctx.remote.ncag`。

| 方法 | 含义 |
|---|---|
| `snapshot` | 项目及其任务、智能体与计数，加上用量、审计链状态、解析出的路由与近期时间线 |
| `trace` | 某个关联标识的审计记录、事件、消息与工具执行 |
| `recentEvents` | 最新平台事件，可全局或按项目读取；数量受 `maxEvents` 限制 |
| `decisions` | 所有待决事项，或某个项目的决策历史 |
| `profiles` | 已注册的智能体档案及其能力 |
| `createProject`、`submitRequest` | 创建工作流并向其提交工作 |
| `pauseProject`、`resumeProject` | 停止或恢复某个项目的调度 |
| `cancelTask`、`reassignTask` | 取消任务或按能力改派 |
| `cancelAgent`、`instructAgent` | 停止某个智能体或对其下达指示 |
| `resolveDecision` | 记录人对某项决策的回答 |

<a id="understand-the-implementation"></a>
## 理解实现

`NcagController` 继承 `TypertRemoteService`，服务键为 `ncagController`，线上命名空间为 `ncag`。每个方法定位到编排器或某个仓储，把失败映射为 `ncag/not-found` 或 `ncag/invalid`，并返回线上记录。记录在 `src/types.ts` 本地声明而不是从平台包导入：宿主记录在结构上满足它们，而浏览器半侧不得把 SQLite、git 或模型端口模块引入自己的程序。命名空间的失败词表位于 `src/codes.ts`，仅宿主面包含它。

页面轮询 `snapshot` 与 `recentEvents`；`trace` 按需读取。被寻址但不存在的记录报告为 `ncag/not-found` 而不是空结果，因此监控不会展示一个并不存在的安静项目。

<a id="known-limitations-and-deferred-work"></a>
## 已知限制与后续工作

- **轮询而非推送。** 页面按间隔重新读取；若要基于 `store.subscribe` 推送，需要新增流方法与转发事件白名单条目。
- **回环信任。** Gateway 将本地用户视为所有者，因此这些读取未应用按对端隔离的资源围栏。
- **完整记录上行。** 项目、任务、智能体与追踪负载未做裁剪；目录很大时可能值得投影。
