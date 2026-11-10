---
description: "NC-AG 持久化存储：SQLite 模式与迁移、类型化仓储、平台事件流与哈希链审计日志。"
kind: "package-library"
---

# @deepseek-ai/dsh-ncag-store

[English](README.md) | 中文

## 摘要

平台数据库：一个 SQLite 文件保存每个项目、请求、需求、档案、智能体、任务、消息、决策、发现、证据行、租约、变更集、工作区、工具执行、用量行、检查点与审计记录。`NcagStore` 拥有连接、有序事件流与审计链；`createRepositories(store.db)` 返回读写各行的类型化仓储。

## 目录

- [使用本包](#use-this-package)
- [模式与迁移](#schema-and-migrations)
- [仓储](#repositories)
- [事件与审计](#events-and-audit)
- [已知限制与后续工作](#known-limitations-and-deferred-work)
- [开发者说明](#dev-note)

-----

<a id="use-this-package"></a>
## 使用本包

```ts
const store = NcagStore.open('/abs/path/platform.db')
const repositories = createRepositories(store.db)
repositories.projects.save(project)
store.emitEvent({ kind: 'project/created', projectId: project.id, summary: 'project created' })
store.recordAudit({ category: 'request', action: 'project-created', actor: 'orchestrator', correlationId, detail: {} })
store.close()
```

`close()` 幂等。打开时会创建缺失目录并设置 `journal_mode = WAL`、`foreign_keys = ON` 与 `busy_timeout = 5000`。

<a id="schema-and-migrations"></a>
## 模式与迁移

模式版本保存在 `PRAGMA user_version`。全新数据库直接创建完整并打戳；较旧的数据库在一个事务内执行有序 `MIGRATIONS`，失败时保留原版本；比构建更新的数据库被拒绝而非降级。每个迁移都记录在 `schema_history`。

JSON 列以文本存储，从而保持物理模式可用纯 SQLite 工具检视。值经 `decodeJson` 解码，对无文档回退的空列直接报错。

<a id="repositories"></a>
## 仓储

| 仓储 | 记录 |
|---|---|
| `projects`、`requests`、`requirements` | 请求侧：项目配置、原始请求、规范化需求 |
| `profiles`、`agents` | 专业化注册表与派生实例 |
| `tasks` | 含状态、依赖、标准与尝试次数的任务图 |
| `messages` | 含投递与确认状态的持久通信日志 |
| `decisions`、`findings`、`evidence` | 决策及其辩论、审查发现、验证证据 |
| `leases`、`changeSets`、`workspaces` | 资源占用、已提交变更、智能体工作树 |
| `toolCalls`、`usage`、`checkpoints`、`toolInvocations` | 执行记录、成本账本、恢复检查点、循环计数 |

`addUsage` 求和用量；`EMPTY_USAGE` 与 `DEFAULT_AUTONOMY` 是无记录时使用的文档化默认值。

<a id="events-and-audit"></a>
## 事件与审计

`emitEvent(input)` 追加一条有序平台事件并通知订阅者；`subscribe` 返回其处置器；`recentEvents(limit, projectId?)` 读取最近的、由旧到新的事件。该流正是控制平面转发给面板的数据。

`AuditLog` 追加一条记录，其哈希覆盖自身内容与前一条记录的哈希，起点为 `AUDIT_GENESIS_HASH`。`verify()` 遍历整条链并报告第一个不匹配的序号，因此被修改、删除或插入的记录从该点起即可检测。通过校验的链是平台的完整性主张。

<a id="known-limitations-and-deferred-work"></a>
## 已知限制与后续工作

- **每进程一个连接。** 编排器拥有它，各仓储共享；第二个进程打开同一文件由 WAL 与 `busy_timeout` 保护，而非租约。多个编排器共用一个平台数据库不是受支持的部署。
- **无压缩。** 事件、证据与工具执行会无界增长。保留策略目前由运维负责；一旦部署需要，该策略应归属本包。

<a id="dev-note"></a>
## 开发者说明

<details>
<summary>维护者工作背景 —— 点击展开</summary>

语句位于 `sql.ts`，因此列变更与其 DDL 一并编辑。`asRows<T>` 是把预处理语句的通用结果转换为调用方行接口的唯一位置；新查询应使用它，而不是在调用点做类型断言。

</details>
