---
description: "NC-AG 领域词汇：带品牌标识符、持久化记录类型、智能体与任务生命周期状态机，以及内置档案注册表。"
kind: "package-library"
---

# @deepseek-ai/dsh-ncag-core

[English](README.md) | 中文

## 摘要

本包是每个 NC-AG 模块所使用的词汇：标识符、记录、生命周期状态机与档案。它不含 I/O、不含服务、不含状态。它的价值在于领域不会漂移：任务状态只能是枚举状态之一，状态迁移只对照一张表校验，标识符带品牌，因此项目标识不会被传入需要任务标识的位置。

## 目录

- [导出](#exports)
- [标识符](#identifiers)
- [生命周期状态机](#lifecycle-machines)
- [档案](#profiles)
- [已知限制与后续工作](#known-limitations-and-deferred-work)
- [开发者说明](#dev-note)

-----

<a id="exports"></a>
## 导出

| 模块 | 内容 |
|---|---|
| `ids.ts` | `ProjectId` … `CorrelationId` 品牌、`newId`、`adoptId`，以及 `IdFor` 族映射 |
| `types.ts` | 记录类型：项目、请求、需求、任务、智能体实例、档案、消息、决策、发现、证据、租约、变更集、工作区、审计记录、事件 |
| `machines.ts` | `AGENT_TRANSITIONS`、`TASK_TRANSITIONS`、`transitionAgent`、`transitionTask`、`canTransition*`、`allowedTransitions`、`NcagStateError` |
| `profiles.ts` | `BUILT_IN_PROFILES`、`builtInProfile`、`validateProfileSeed`、`profilesForCapability`、`tierForCapability` |

<a id="identifiers"></a>
## 标识符

`newId(kind)` 生成带前缀的 UUID（`prj_…`、`tsk_…`、`agt_…`），并返回该族对应的确切品牌，调用点因此无需类型断言。`adoptId(kind, text)` 为从数据库读回的文本重建品牌，并拒绝前缀不匹配的文本。这一拒绝正是损坏或不匹配的标识符会在读取时失败、而不是继续传播的原因。

<a id="lifecycle-machines"></a>
## 生命周期状态机

`transitionAgent(from, to)` 与 `transitionTask(from, to)` 返回请求的状态，或抛出 `NcagStateError`，其中同时指出两个状态与状态机名称。自迁移返回原状态。`canTransitionAgent` 与 `canTransitionTask` 只回答问题而不抛出，供规划器与面板使用。`allowedTransitions(machine, state)` 返回完整的目标列表。

表格即权威。`STOPPED` 没有出边，`FAILED` 只能转入 `RECOVERING` 或 `STOPPING`，任务 `COMPLETED` 是终态。平台中每一次持久化的状态变更都经由这些函数之一。

<a id="profiles"></a>
## 档案

注册表内置十六个种子。`builtInProfile(seed)` 附加稳定的 `prf_builtin_<key>` 标识。`validateProfileSeed` 强制键格式、唯一性，以及能力、身份、职责与限制的存在，因此新建档案在使用前就是完整的。

<a id="known-limitations-and-deferred-work"></a>
## 已知限制与后续工作

- **档案是编译期表格。** 注册表是 `profiles.ts` 中的数据；部署可通过 `AgentRegistry.create` 在运行时新增档案并持久化，因此专业化无需修改代码。文件式档案包将消除最后一个编译期接口，但目前尚无使用方。

<a id="dev-note"></a>
## 开发者说明

<details>
<summary>维护者工作背景 —— 点击展开</summary>

新增状态需要同时修改 `types.ts` 与 `machines.ts`；只改一处会产生一个可达却无法离开的状态，或一个类型系统拒绝的迁移。新增档案只需修改 `profiles.ts`。

</details>
