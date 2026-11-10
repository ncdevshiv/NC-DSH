---
description: "运维 NC-AG：控制平面 API、运维控制、预算处理，以及运行中平台的恢复流程。"
kind: "guide"
---

# NC-AG 运维

[English](operation.md) | 中文

## 摘要

控制平面是由 [`ncag-dashboard`](../../packages/ncag/ncag-dashboard/README.zh.md) 拥有的进程内 HTTP 服务。它提供一个面板文档、一条服务器事件流，以及面板自身调用的 API。每个路由都执行它所声明的操作：面板只渲染返回的数据，每个控件都调用与面向模型工具相同的编排器方法。

## 目录

- [启动与停止](#start-and-stop)
- [API 参考](#api-reference)
- [运维流程](#operator-procedures)
- [预算处理](#budget-handling)
- [恢复流程](#recovery-procedures)
- [健康检查](#health-checks)

-----

<a id="start-and-stop"></a>
## 启动与停止

控制平面随平台服务启动，监听 `NCAG_PORT`（0 选择空闲端口）与 `NCAG_HOST`。独立运维进程可以直接在数据库上启动同一平台：

```ts
import { runPlatform } from '@deepseek-ai/dsh-ncag-dashboard'

const platform = await runPlatform(process.env, llmPort, routeRegistry)
console.log(platform.url)
await platform.stop()
```

`llmPort` 是 [`ncag-runtime`](../../packages/ncag/ncag-runtime/README.zh.md) 模型端口。它为 `null` 时平台仍会启动，但每个需要模型的任务都会报告缺失路由，而不是执行。

停止是优雅的：编排器记录执行检查点、中止进行中的智能体并关闭数据库。调用两次 `stop` 是无操作。

<a id="api-reference"></a>
## API 参考

配置了令牌时，以下每个路由都需要 `Authorization: Bearer $NCAG_TOKEN`；面板以 `?token=` 为事件流传递它，因为浏览器 `EventSource` 无法设置首部。

| 方法与路径 | 操作 |
|---|---|
| `GET /` | 面板文档 |
| `GET /events` | 平台事件的服务器发送流 |
| `GET /api/health` | 审计链状态、项目数与活跃智能体数；链断裂时 `500` |
| `GET /api/snapshot` | 完整平台快照：含任务与智能体的项目、用量、事件、审计状态、路由、档案数 |
| `GET /api/projects` | 所有项目 |
| `POST /api/projects` | 创建项目（`name`、`description`、`repositoryPath`，可选 `baseBranch`、`instructions`） |
| `POST /api/projects/:id/requests` | 提交请求；返回计划与创建的任务 |
| `GET /api/projects/:id/tasks` | 项目任务 |
| `GET /api/projects/:id/agents` | 项目智能体实例 |
| `GET /api/projects/:id/messages` | 项目持久消息日志 |
| `GET /api/projects/:id/decisions` | 项目决策记录 |
| `GET /api/projects/:id/leases` | 活跃资源租约 |
| `GET /api/projects/:id/changesets` | 已提交与已集成的变更集 |
| `GET /api/projects/:id/events` | 项目最近的平台事件 |
| `GET /api/tasks/:id/changesets` | 某个任务提交的变更集 |
| `GET /api/tasks/:id/evidence` | 某个任务记录的验证证据，含每个标准的命令、退出码与输出 |
| `GET /api/tasks/:id/findings` | 某个任务产生的审查发现，含严重度、工件与状态 |
| `POST /api/projects/:id/pause` | 暂停项目并记录原因 |
| `POST /api/projects/:id/resume` | 恢复并调度项目已持有的工作 |
| `POST /api/tasks/:id/cancel` | 取消任务并释放其智能体的租约 |
| `POST /api/decisions/:id/resolve` | 记录决策结果，或用户的覆盖及其理由 |
| `POST /api/agents/:id/instruct` | 通过持久消息日志向一个智能体发送运维指令 |
| `POST /api/agents/:id/cancel` | 取消一个活跃智能体：中止它、释放其租约、把其任务返回队列 |
| `POST /api/tasks/:id/reassign` | 以另一种能力重新排队一个任务（`capability` 与可选 `reason`） |
| `GET /api/projects/:id/waiters` | 排队等待各被占路径的智能体，含其任务与声明的原因 |
| `GET /api/projects/:id/{profiles,requests,requirements,reviews}` | 已注册档案、项目请求、其规范化需求与审查运行 |

令牌缺失或错误返回 `401`。未知路由返回 `404` 并给出所请求的路径。失败的操作返回 `500` 与编排器抛出的消息；平台绝不对抛出异常的操作报告成功。

<a id="operator-procedures"></a>
## 运维流程

**维护前暂停。** `POST /api/projects/:id/pause` 停止新的调度；运行中的任务会自然结束。用 `POST .../resume` 恢复，它会立即调度项目已持有的就绪工作。

**停止失控任务。** `POST /api/tasks/:id/cancel` 中止智能体、释放其租约，并把取消原因记录到任务与审计链。取消任务不会删除其工作；工作区与变更集仍可检视。

**取消一个智能体。** `POST /api/agents/:id/cancel` 中止正在运行的实例、释放其租约，并把任务重新排队，让另一次尝试接手；任务本身与已完成的工作都不会丢失。

**重新分配工作。** `POST /api/tasks/:id/reassign` 以另一种能力重新排队一个任务，编排器据新档案的权限重新计算所需权限。

**发送指令。** `POST /api/agents/:id/instruct` 把文本写入项目消息日志、以该智能体为收件人，并要求确认。它会在智能体的下一个步骤到达，投递情况在时间线可见。

**回答决策。** 决策中心显示结构化异议：请求内容、平台主张、冲突、建议、对比过的备选方案、证据与确切问题。接受会记录建议；记录备选方案会记录被拒绝的建议、用户理由、一条项目约束，并解除该决策阻塞的任务。

<a id="budget-handling"></a>
## 预算处理

每次模型调用都按配置的路由价格计价并追加到用量账本。每次记录后编排器都会重新检查项目上限，因此超过 `maxCostUsd` 的项目在跨过上限的那次调用完成时即被暂停，而不是等到下次调度。随后它会提出一个 `user-reserved` 决策请求追加支出。在该决策被回答之前不会执行任何工作，项目通过 `resumeProject` 或显式批准流程恢复。

预算状态是持久的：`GET /api/snapshot` 报告每个项目已花费金额，暂停状态在重启后仍保留。

<a id="recovery-procedures"></a>
## 恢复流程

**停滞的智能体。** 健康观察者报告心跳或持久进度停止的智能体。编排器中止其工作、释放其租约、记录证据、把任务连同一次尝试重新放回就绪队列，并让智能体经由生命周期状态机停止。超过 `maxRetries` 时任务失败并保留原因，仍然可见。

**重复的工具调用。** 同一智能体的 `NCAG_REPEATED_TOOL_THRESHOLD` 次相同调用（同工具、同参数）会发出带指纹与尝试次数的 `recovery/stall-detected` 事件，因此循环在耗尽整个步数预算前就可见。

**提供方失败。** 模型调用经由 harness LLM 服务失败，该服务拥有提供方重试策略。NC-AG 记录智能体上的失败，恢复路径把运行抛出异常的智能体当作停滞智能体处理。

**重启。** 启动时运行 `recoverAfterRestart`：停止任务已完成的智能体、把被中断的任务放回就绪队列、重试待投递消息，并记录一条 `restart-recovered` 审计项。它绝不重跑已完成工作；重启后的状态迁移计数等于重启前。

<a id="health-checks"></a>
## 健康检查

`GET /api/health` 在链校验通过时返回 `status: "ok"` 与审计状态，否则返回 `status: "audit-chain-broken"`、HTTP `500` 与首个断裂序号。把编排器或监控探针指向它。只有在平台之外修改或删除记录时链才会断裂。
