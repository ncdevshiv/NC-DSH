---
description: "NC-AG 门面包：以单一导入获得平台服务、编排库、存储与控制平面。"
kind: "package-library"
---

# @deepseek-ai/dsh-ncag

[English](README.md) | 中文

## 摘要

门面包重新导出平台各包，使使用方只需导入一个名称：服务插件、工具工厂、编排器、领域词汇、存储、运行时与控制平面。它不添加任何行为或自身配置。

## 目录

- [重新导出的内容](#what-it-re-exports)
- [已知限制与后续工作](#known-limitations-and-deferred-work)

-----

<a id="what-it-re-exports"></a>
## 重新导出的内容

| 来源 | 内容 |
|---|---|
| `@deepseek-ai/dsh-tool-ncag` | `apply`、`inject`、`name`、`createNcagTools` |
| `@deepseek-ai/dsh-ncag-service` | 默认服务类、`Config`、`name`、`inject` |
| `@deepseek-ai/dsh-ncag-orchestrator` | 编排器、规划器、DAG、审查、验证、租约、Git、决策、成本、恢复 |
| `@deepseek-ai/dsh-ncag-core` | 标识符、记录、生命周期状态机、档案 |
| `@deepseek-ai/dsh-ncag-store` | `NcagStore`、仓储、审计、模式 |
| `@deepseek-ai/dsh-ncag-runtime` | 端口、工具、提示词装配、循环、harness 模型端口 |
| `@deepseek-ai/dsh-ncag-dashboard` | `startDashboard`、`runPlatform`、面板文档 |

<a id="known-limitations-and-deferred-work"></a>
## 已知限制与后续工作

- **所有面向一个名称。** 想要服务插件的使用方导入门面包，也会加载各库；只想要数据模型的使用方直接导入 `ncag-core`。
