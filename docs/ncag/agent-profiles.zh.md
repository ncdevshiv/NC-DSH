---
description: "十六个内置 NC-AG 智能体专业化、各自职责，以及如何在不扩散智能体的前提下注册新档案。"
kind: "reference"
---

# NC-AG 智能体档案

[English](agent-profiles.md) | 中文

## 摘要

档案是可复用的专业化：身份、性格、专长、能力、允许的工具、权限、职责、限制、必需输入、交付物、质量标准、升级规则与绩效指标。档案是数据。规划按能力选择一个，编排器为每个任务派生一个实例。[`ncag-core/profiles.ts`](../../packages/ncag/ncag-core/src/profiles.ts) 是注册表。

## 目录

- [内置档案](#built-in-profiles)
- [能力与层级](#capabilities-and-tiers)
- [新增档案](#adding-a-profile)

-----

<a id="built-in-profiles"></a>
## 内置档案

| 键 | 角色 | 能力 | 允许工具 | 层级 |
|---|---|---|---|---|
| `general-purpose` | 杂项工作、调研、支持 | research, analysis, general-implementation | read, glob, grep, write, edit | standard |
| `system-architect` | 边界、接口、架构记录 | architecture, interface-design, dependency-analysis, architecture-review | read, glob, grep, write, edit | reasoning |
| `backend-engineer` | 服务、业务逻辑、授权 | backend-implementation, server-testing, persistence | read, glob, grep, write, edit, shell | standard |
| `frontend-engineer` | 界面、状态、可访问性 | frontend-implementation, ui-accessibility, frontend-testing | read, glob, grep, write, edit, shell | standard |
| `api-engineer` | 模式、端点、版本化 | api-contract, api-implementation, api-validation | read, glob, grep, write, edit, shell | reasoning |
| `database-engineer` | 模式、迁移、查询性能 | database-design, migrations, query-optimization | read, glob, grep, write, edit, shell | standard |
| `devops-engineer` | 构建、部署、CI、健康 | devops, deployment, observability, operational-health | read, glob, grep, write, edit, shell | standard |
| `documentation-engineer` | 产品、API、运维文档 | documentation, documentation-review | read, glob, grep, write, edit | fast |
| `qa-engineer` | 功能、回归、验收验证 | test-planning, functional-verification, regression-testing | read, glob, grep, write, edit, shell | standard |
| `e2e-test-engineer` | 旅程、工作流、失败路径 | e2e-testing, journey-testing, failure-path-testing | read, glob, grep, write, edit, shell | standard |
| `security-auditor` | 认证、机密、权限、依赖风险 | security-review, threat-analysis, permission-audit | read, glob, grep, shell | reasoning |
| `code-reviewer` | 正确性、可维护性、回归 | code-review, quality-review | read, glob, grep, shell | reasoning |
| `critiquer` | 假设、场景、替代方案 | critique, assumption-challenge, alternative-analysis | read, glob, grep | reasoning |
| `integration-engineer` | 合并协调、语义冲突 | integration, merge-conflict-resolution, integration-validation | read, glob, grep, write, edit, shell | reasoning |
| `performance-engineer` | 度量、负载、优化 | performance-analysis, load-testing, optimization | read, glob, grep, shell | standard |
| `product-analyst` | 意图、需求、可追溯性 | requirements-analysis, journey-definition, acceptance-definition | read, glob, grep | standard |

档案持有的每个权限都在工具运行前被强制执行：`workspace:read` 覆盖 `read`、`glob` 与 `grep`；`workspace:write` 覆盖 `write` 与 `edit`；`shell:execute` 覆盖 shell 工具。列出工具但缺少权限的档案无法执行它，拒绝会被审计。

<a id="capabilities-and-tiers"></a>
## 能力与层级

规划把任务的 `requiredCapability` 与注册表匹配。执行层级取自首个提供该能力的档案，随后路由在首选提供方中选择服务该层级的最便宜路由。这就是标准层级角色落在廉价模型、架构与审查落在推理模型的方式，无需任何按任务的模型配置。

<a id="adding-a-profile"></a>
## 新增档案

`AgentRegistry.create(seed)` 先校验种子：键必须为 3-40 个小写字母数字或连字符且不得冲突，至少需要一个能力，必须陈述身份与职责，必须声明限制。注册后的档案是持久的，重启后依然存在。

```ts
const profile = orchestrator.agents.create({
  key: 'mobile-engineer',
  role: 'Mobile engineer',
  identity: 'You own the mobile client.',
  personality: 'Platform-pragmatic; you test on a real target.',
  expertise: ['iOS', 'Android'],
  capabilities: ['mobile-implementation'],
  toolNames: ['read', 'write', 'edit', 'shell', 'request_help', 'hand_off'],
  responsibilities: ['Implement the mobile client against the approved contract.'],
  restrictions: ['Do not change the published API contract.'],
  permissions: ['workspace:read', 'workspace:write', 'shell:execute', 'communication:send', 'resource:lease'],
  requiredInputs: ['approved interface contract'],
  deliverables: ['implementation', 'device test output'],
  qualityStandard: 'Every screen is exercised on a device or emulator with recorded output.',
  escalationRules: ['Escalate when the approved contract cannot express the required behavior.'],
  performanceMetrics: ['device test pass rate'],
  defaultTier: 'standard',
})
```

在新增之前，编排器会检查是否已有档案提供该能力，因此任务绝不会派生冗余专业化。

每个内置档案都列出两个协调工具，因为无法求助或交接工作的智能体只能独自承担：`request_help` 需要 `communication:send` 权限，`hand_off` 需要同一权限。没有该权限的档案仍可使用它声明的工作区与 shell 工具，但无法联系另一个智能体。
