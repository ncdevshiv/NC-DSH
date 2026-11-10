---
description: "挂载 NC-AG 自主多智能体平台及其面向模型工具的 dsh bundle。"
kind: "package-bundle"
---

# @deepseek-ai/dsh-bundle-ncag

[English](README.md) | 中文

## 摘要

本 bundle 在 harness 组合旁插入两行：NC-AG 平台服务，以及暴露 `ncag_project`、`ncag_request`、`ncag_status` 与 `ncag_decide` 的工具插件。编排器经由 harness LLM 服务派生自己的智能体，因此每个 NC-AG 角色都运行在与 dsh 其余部分相同的已配置提供方路由上。

## 目录

- [使用本 bundle](#use-this-bundle)
- [配置](#configuration)
- [不替换的内容](#what-it-does-not-replace)
- [已知限制与后续工作](#known-limitations-and-deferred-work)
- [开发者说明](#dev-note)

-----

<a id="use-this-bundle"></a>
## 使用本 bundle

```bash
dsh --profile web --patch packages/bundle/ncag/cordis.patch.yml
```

`ncag` 档案叠加 `dsh-base`、Web 应用与本 bundle，因此 `dsh --profile ncag web` 是用一个名称的同一棵树。

<a id="configuration"></a>
## 配置

| 变量 | 必需 | 含义 |
|---|---|---|
| `NCAG_DATABASE_PATH` | 否 | 平台数据库绝对路径（默认位于 harness home 下） |
| `NCAG_WORKSPACE_ROOT` | 否 | 智能体工作区绝对目录（默认位于 harness home 下） |
| `NCAG_MODEL_PRICING` | 否 | JSON 价格覆盖；为空时只记录 token，不计 USD |
| `NCAG_PREFERRED_PROVIDERS` | 否 | 按优先级排列的提供方名称 |
| `NCAG_MAX_CONCURRENT_AGENTS` | 否 | 每项目活跃智能体上限 |
| `NCAG_MAX_STEPS` | 否 | 每任务模型步数 |
| `NCAG_MAX_OUTPUT_TOKENS` | 否 | 每次调用输出上限 |

平台无需任何环境变量即可加载。在 NC-AG 设置页指定按层级覆盖之前，模型路由跟随用户主模型。

<a id="what-it-does-not-replace"></a>
## 不替换的内容

不替换任何内容。补丁只插入行；它不替换文件系统提供方、工具注册表、shell、模型适配器或任何其他基础行。NC-AG 与 harness 智能体循环并行运行，并以 harness LLM 服务作为其模型端口。

<a id="known-limitations-and-deferred-work"></a>
## 已知限制与后续工作

- **控制平面是独立的。** 本 bundle 挂载平台服务；通过 HTTP 提供面板服务是 [`ncag-dashboard`](../../ncag/ncag-dashboard/README.zh.md) 的职责，独立部署通过 `runPlatform` 启动它。
- **档案模板随 Web 应用一起提供。** 纯无头的 NC-AG 档案将需要不含 Web bundle 的行列表；`ncag` 模板组合 Web 应用，因为那是运维使用的组合。

<a id="dev-note"></a>
## 开发者说明

<details>
<summary>维护者工作背景 —— 点击展开</summary>

两行分属不同包，使服务可以在不加载工具的情况下组合（无头平台），工具也可以针对在别处挂载的平台服务组合。

</details>
