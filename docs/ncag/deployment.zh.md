---
description: "如何安装、配置并启动 NC-AG：必需环境、模型价格、凭据、数据库与第一个项目。"
kind: "guide"
---

# NC-AG 部署

[English](deployment.md) | 中文

## 摘要

NC-AG 运行在 `dsh` 档案内。它需要三样东西：平台数据库路径、工作区目录，以及它可能使用的每条模型路由的价格。模型凭据属于 harness，因此 NC-AG 从不存储它们。本页配置三者、挂载平台、验证启动并创建第一个项目。

## 目录

- [要求](#requirements)
- [必需配置](#required-configuration)
- [模型路由与价格](#model-routes-and-prices)
- [凭据](#credentials)
- [挂载平台](#mount-the-platform)
- [验证启动](#verify-the-boot)
- [第一个项目](#first-project)
- [升级与模式迁移](#upgrades-and-schema-migrations)

-----

<a id="requirements"></a>
## 要求

| 要求 | 原因 |
|---|---|
| Node 22.19+ 或 24+ | harness 引擎范围；平台使用 `node:sqlite` |
| 已注册的 harness 工作区 | 未显式给出仓库路径的项目以其为锚点 |
| `PATH` 上的 `git`，或设置 `GIT_EXECUTABLE` | 隔离工作树、变更集与集成都运行真实 git |
| shell（Windows 上 `powershell.exe`，其他平台 `pwsh`），或 `NCAG_SHELL` | 验证命令与 `shell` 工具 |
| 具备凭据的模型提供方 | 智能体经由 harness LLM 服务执行 |

NC-AG 从不通过 shell 字符串调用 git：它一次性解析可执行文件（`GIT_EXECUTABLE`，否则 `git`）并以参数数组运行，因此含空格的路径也能工作。

<a id="required-configuration"></a>
## 必需配置

| 变量 | 必需 | 含义 |
|---|---|---|
| `NCAG_DATABASE_PATH` | 否 | 平台 SQLite 数据库绝对路径；默认位于 harness home 下 |
| `NCAG_WORKSPACE_ROOT` | 否 | 智能体工作树绝对目录；默认位于 harness home 下 |
| `NCAG_MODEL_PRICING` | 否 | JSON 价格覆盖；为空时只记录 token，不计 USD |
| `NCAG_PREFERRED_PROVIDERS` | 否 | 按优先级排列的提供方名称，逗号分隔 |
| `NCAG_MAX_CONCURRENT_AGENTS` | 否 | 每项目默认活跃智能体上限（4） |
| `NCAG_MAX_STEPS` | 否 | 循环停止前的每任务模型步数（24） |
| `NCAG_MAX_OUTPUT_TOKENS` | 否 | 每次模型调用输出上限（4096） |
| `NCAG_SHELL` | 否 | 命令与验证使用的 shell 可执行文件 |
| `GIT_EXECUTABLE` | 否 | `git` 不在 `PATH` 时使用的 git 可执行文件 |
| `NCAG_PORT` | 否 | 控制平面端口（`0` 选择空闲端口） |
| `NCAG_HOST` | 否 | 控制平面绑定地址（`127.0.0.1`） |
| `NCAG_TOKEN` | 否 | API 调用所需的 bearer 令牌；控制平面可从其他机器访问时必须设置 |

缺少值现在回退到 harness home，因此随包发布的 Web preset 与 bundle 无需环境变量即可启动。平台绝不臆造路由：未定价路由仍以只记 token 的方式运行，模型路由来自用户主模型，除非设置页指定覆盖。

<a id="model-routes-and-prices"></a>
## 模型路由与价格

`NCAG_MODEL_PRICING` 是一个 JSON 数组：

```json
[
  { "provider": "deepseek", "model": "deepseek-chat", "inputPerMillion": 0.27, "outputPerMillion": 1.1, "tier": "standard" },
  { "provider": "deepseek", "model": "deepseek-reasoner", "inputPerMillion": 0.55, "outputPerMillion": 1.6, "tier": "reasoning" }
]
```

`tier` 可选；缺省时路由根据模型标识推断层级。价格是可选的：没有价格条目的路由仍会运行并记录 token 数，其 USD 成本保持未知。为你希望预算上限执行的路由定价。

- 已定价路由向项目账本贡献 USD；未定价路由只贡献 token。预算上限在你于 NC-AG 设置页设置后才生效。
- 档案要求层级，而非模型。路由对每个层级使用用户主模型，直到设置页指定覆盖；需要自行发现路由时优先 `NCAG_PREFERRED_PROVIDERS`。

<a id="credentials"></a>
## 凭据

模型凭据属于 harness 凭据服务。在环境中设置 `DEEPSEEK_API_KEY`，或通过 Web 模型页存储。NC-AG 从不把凭据写入数据库、消息或审计日志；它向 harness LLM 服务请求流，由 harness 解析凭据。

<a id="mount-the-platform"></a>
## 挂载平台

```bash
dsh --profile web --patch packages/bundle/ncag/cordis.patch.yml
```

该补丁插入两行：平台服务（`@deepseek-ai/dsh-ncag-service`）与工具插件（`@deepseek-ai/dsh-tool-ncag`）。`ncag` 档案组合 `dsh-base`、Web 应用与该 bundle，因此 `dsh --profile ncag web` 是同一棵树。首次启动会初始化 `$DSH_HOME/profiles/ncag`。

随发行版交付的 Web 应用也无需补丁档案即可挂载该平台：在 Agent 选择器中选择 **NC-AG 模式**。其 preset 在隔离域中插入同样两行，因此平台随选择该模式的会话结束，而不会发布为宿主服务。它读取 `NCAG_DATABASE_PATH`、`NCAG_WORKSPACE_ROOT`、`NCAG_MODEL_PRICING` 及可选调优变量，两个路径默认位于 `$DSH_HOME/ncag`，并按绝对数据库路径与进程内其他挂载共享同一个编排器。

<a id="verify-the-boot"></a>
## 验证启动

```bash
dsh --profile ncag --dump-config | grep -A12 "dsh-bundle-ncag"
```

输出必须显示含三个必需配置值的 `ncag` 行与 `tools-ncag` 行。若服务导入失败，启动日志会打印 `ncag (@deepseek-ai/dsh-ncag-service): failed to import`；若模型端口缺失，harness LLM 插件会打印 `MISSING_CREDENTIAL`，此后 NC-AG 任务会报告缺失路由，而不是运行。

<a id="first-project"></a>
## 第一个项目

通过面向模型的工具：

```text
ncag_project  name: checkout-service, description: a small order API, repositoryPath: /abs/path/repo
ncag_request  projectId: <id from ncag_project>, text: "Add a REST endpoint that returns an order by id, with tests."
ncag_status   projectId: <id>
```

通过控制平面（设置 `NCAG_PORT` 时）：

```bash
curl -X POST "$URL/api/projects" -H "authorization: Bearer $NCAG_TOKEN" -H "content-type: application/json" \
  -d '{"name":"checkout-service","description":"order API","repositoryPath":"/abs/path/repo"}'
```

在 NC-AG 自行创建的仓库上创建的项目会获得初始提交与配置的基线分支，因为工作树需要一个提交作为分支起点。在既有仓库上创建的项目按原样检视：NC-AG 绝不重写可用仓库以迎合偏好。

<a id="upgrades-and-schema-migrations"></a>
## 升级与模式迁移

平台数据库携带 `PRAGMA user_version`。全新数据库按当前版本创建；较旧数据库在一个事务内由有序迁移列表前向迁移；比构建更新的数据库被拒绝而非降级。备份即在编排器停止时复制数据库文件；该布局意味着一个文件及其 WAL 就是全部状态。
