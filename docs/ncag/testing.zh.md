---
description: "NC-AG 测试套件：每个规范证明什么、脚本化模型如何驱动真实执行路径，以及如何运行该套件。"
kind: "guide"
---

# NC-AG 测试

[English](testing.md) | 中文

## 摘要

NC-AG 套件演练真实执行路径：真实 Git 工作树、真实文件工具、真实租约、真实审查与验证、真实集成，每个测试 harness 一个 SQLite 数据库。只有模型调用是脚本化的，因为那是唯一花钱且需要提供方的边界。本页列出每个规范证明什么，以及如何运行。

## 目录

- [运行套件](#run-the-suite)
- [脚本化模型](#the-scripted-model)
- [场景覆盖](#scenario-coverage)
- [套件未覆盖的内容](#what-the-suite-does-not-cover)

-----

<a id="run-the-suite"></a>
## 运行套件

```bash
npx vitest run packages/ncag
```

| 规范 | 证明 |
|---|---|
| `packages/ncag/ncag-orchestrator/tests/scenario-a.spec.ts` | 请求被规划、在真实工作树中执行、审查、以已执行检查验证，并在审计链上记录为已完成任务 |
| `packages/ncag/ncag-orchestrator/tests/scenarios.spec.ts` | 两个并发项目保持隔离；被占用路径拒绝第二个写入者并指出持有者；重复工具调用触发循环事件；重大异议被记录且用户覆盖被应用；验证失败使任务停止；重启在不重复工作的情况下保留状态；成本上限暂停项目并请求决策；形似指令的工作区内容被框定为数据；健康扫描会在不重新分配已完成任务的前提下停用已完成的智能体；已批准的变更被集成到基础分支，不依赖宿主机的全局 git 身份；咨询由具名专家作答，其答复写入项目上下文 |
| `packages/ncag/ncag-orchestrator/tests/operators.spec.ts` | 取消活跃智能体会把其任务返回队列；重新分配任务会把它转到另一种能力并采用该档案的权限；用户决策会解除受影响任务的阻塞，无论建议被接受还是被覆盖 |
| `packages/ncag/ncag-dashboard/tests/control-plane.spec.ts` | 面板被提供、未认证调用被拒绝，API 操作创建项目并读回，且文档中的每个控件都调用服务器提供的路由 |

每个规范启动自己的临时仓库与数据库，并在拆卸时删除二者。只有在单独运行时才通过的规范本身就是缺陷。

<a id="the-scripted-model"></a>

## 脚本化模型

`ScriptedModel` 实现运行时消费的 `LlmPort`。脚本是按智能体身份短语划分的步骤列表；每一步要么是工具调用，要么是最终回答。编排器驱动与生产相同的循环，因此每个脚本步骤都会针对真实工作区执行真实工具。

<a id="scenario-coverage"></a>

## 场景覆盖

| 场景 | 规范 |
|---|---|
| 简单请求 | `scenario-a.spec.ts` |
| 全栈请求 | 能力规则选出前端、API、数据库与后端任务并构建依赖图 |
| 多个并发请求 | `scenarios.spec.ts` B/C |
| 共享文件冲突 | `scenarios.spec.ts` D 证明租约拒绝第二个写入者；`operators.spec.ts` 让两个工作树编辑同一行，经由真实集成管理器运行，并证明第二个变更集被报告为文本 `CONFLICT`，而基础分支只保留第一个变更 |
| 智能体失败与卡死智能体 | `scenarios.spec.ts` E/F |
| 求助请求 | `scenarios.spec.ts` G：工作中的智能体调用 `request_help`，所需专业化的真实辅助实例在该工作区上运行循环，其答复在请求方的运行中返回 |
| 辅助预算 | `scenarios.spec.ts` G2：超过项目 `maxAgentsPerTask` 的第二次求助请求被拒绝并指出该上限，因此专家不会被递归派生 |
| 智能体交接 | `scenarios.spec.ts` G3：智能体调用 `hand_off`，`HANDOFF` 交接包被记录，继任者在继承的工作区中继续，编辑其前任写入的文件 |
| 用户异议与覆盖 | `scenarios.spec.ts` H/I |
| 审查异议 | `scenarios.spec.ts` J |
| 编排器重启 | `scenarios.spec.ts` K |
| 预算耗尽 | `scenarios.spec.ts` L |
| 长时间运行恢复 | `scenarios.spec.ts` M 在智能体正在处理任务时中断它，重启平台，并证明被中断的任务带着完好依赖回到队列，而已完成的任务不再产生新的迁移，恢复的一轮恰好完成它一次 |
| 不可信指令 | `scenarios.spec.ts` N |
| 已完成智能体的扫描与集成 | `scenarios.spec.ts` O/P |
| 咨询请求 | `scenarios.spec.ts` Q |
| 最终交付 | `scenario-a.spec.ts` 验证证据 |

<a id="what-the-suite-does-not-cover"></a>

## 套件未覆盖的内容

- **真实模型提供方。** 套件按设计在无凭据下运行。真实提供方行为由仓库端到端套件覆盖，该套件在无密钥时自动跳过。
- **浏览器驱动的面板交互。** 控制平面规范调用面板所调用的 API；文档本身只断言所提供的内容，而非渲染像素。
