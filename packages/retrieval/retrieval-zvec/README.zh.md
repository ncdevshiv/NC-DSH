---
description: "基于 zvec 的工作区检索提供方：仓库本地集合、原生全文排序，以及可选的本地嵌入。"
kind: "package-reference"
---

# @deepseek-ai/dsh-retrieval-zvec

[English](README.md) | 中文

## 摘要

使用本包为组合提供真正的原生工作区检索引擎。它用官方 [`@zvec/zvec`](https://github.com/alibaba/zvec) Node 绑定注册 `ctx.retrieval`：每个工作区在 `<workspace>/.dsh/retrieval` 下拥有一个集合，支持原生全文索引、标量过滤、稠密向量以及倒数排名融合的混合搜索。词法搜索无需嵌入模型即可工作；语义与混合搜索使用调用方向量，或使用显式配置的本地 `fastembed` 模型，其权重同样留在该工作区目录内。随发行版的组合会启用该提供方但关闭嵌入，因此默认检索不会下载模型。

## 目录

- [使用本包](#use-this-package)
- [理解实现](#understand-the-implementation)
- [进一步探索](#further-exploration)
- [模型体验](#model-experience)
- [已知限制与后续工作](#known-limitations-and-deferred-work)
- [开发说明](#dev-note)

-----

<a id="use-this-package"></a>
## 使用本包

在组合中存在工作区消费者之后挂载它。随发行版的 `dsh-base` 组合以 `embedding: false` 挂载它，并同时挂载 `dsh-tool-retrieval` 与 `dsh-retrieval-session-index`。

### 最小配置

```yaml
- name: '@deepseek-ai/dsh-retrieval-zvec'
  config:
    # Defaults: dataDir `.dsh`, collection `retrieval`, embedding false.
    vectorDimension: 384
```

| 字段 | 默认值 | 含义 |
|---|---|---|
| `dataDir` | `.dsh` | 存放集合、日志与模型缓存的仓库本地目录 |
| `collectionName` | `retrieval` | `dataDir` 内的 zvec 集合目录名 |
| `vectorDimension` | 省略 | 以给定向维度启用稠密向量；省略则仅词法搜索 |
| `embedding` | 省略 | `false` 关闭本地嵌入；对象形式选择 `fastembed` 模型、`maxLength` 与 `batchSize` |
| `maxLimit` | `100` | 单次搜索接受的最大分页上限 |
| `queryThreads` / `optimizeThreads` | 省略 | zvec 原生线程数；省略时由引擎决定 |
| `logLevel` | `ERROR` | zvec 原生日志级别，写入 `<dataDir>/<collection>-logs` |

当设置 `embedding` 而未设置 `vectorDimension` 时，提供方使用模型维度（默认英文 BGE small v1.5 为 384）。显式维度必须与所选模型一致。

### 仓库本地数据

提供方从不写到传入工作区根目录之外。集合路径、原生日志目录与嵌入缓存都位于 `<workspace>/<dataDir>`。本仓库的 `.gitignore` 忽略 `.dsh/`；部署方也应同样忽略或排除它。首次打开时创建目录树；提供方绝不回退到进程工作目录。

### 语义与生命周期

词法搜索把查询作为一个字面短语引用，因此 FTS 语法无法逃逸文本。过滤条件会变成作用于 `scope`、`path`、`session_id`、`seq`、`type`、`surface` 与 `time` 的 zvec 布尔表达式。语义搜索需要 `vectorDimension`；混合搜索运行一个 FTS 子查询和一个向量子查询，并以 RRF 排名常数 60 融合。写入按提供方串行，zvec 侧为同步调用；`flush()` 等待已接受写入链，`close()` 在关闭所有集合前排空它。

zvec 对每个集合路径持有独占可写锁。同一集合的第二次可写打开——无论在进程内还是其他进程——都会显式失败，因此同一时间只有一个 DSH 进程拥有某个工作区集合。

### 失败

提供方失败为带服务稳定错误码的 `RetrievalError`。集合缺失或被锁定时报告 `RETRIEVAL_PROVIDER_FAILED`；没有维度或嵌入器的语义请求报告 `RETRIEVAL_SEMANTIC_DISABLED`；非法向量、游标或分页上限报告 `RETRIEVAL_INVALID_REQUEST`。

-----

<a id="understand-the-implementation"></a>
## 理解实现

<details>
<summary>实现内部细节——点击展开</summary>

- **单一介质、单一写入者。** zvec 的原生独占锁就是所有权原语；提供方在其上为每个工作区维护集合句柄。
- **仓库身份。** 调用方提供绝对工作区根目录；提供方解析它、确认其为目录，并在其下构建全部路径。
- **可选语义能力。** 仅当配置了维度时集合模式才包含向量字段，因此仅词法部署不承担向量内存，也不会意外接受向量。
- **串行化已接受写入。** 每个变更进入同一条承诺链；失败的操作不会污染之后的已接受操作。

### 源码地图

| 文件 | 角色 |
|---|---|
| [`src/index.ts`](src/index.ts) | 提供方配置、集合生命周期、过滤、搜索模式与可选嵌入运行时 |

</details>

-----

<a id="further-exploration"></a>
## 进一步探索

- [dsh-retrieval](../retrieval/README.zh.md)——本提供方实现的服务定义。
- [dsh-retrieval-session-index](../retrieval-session-index/README.zh.md)——将实时会话事件镜像进本提供方的消费者。
- [dsh-tool-retrieval](../tool-retrieval/README.zh.md)——面向模型的工作区搜索与索引工具。

-----

<a id="model-experience"></a>
## 模型体验

间接地通过 `dsh-tool-retrieval`：由它把工作区命中与失败渲染为有界工具结果。

#### KV 缓存影响

不直接导致失效；请求前缀变化由上述消费者自行负责。

## 已知限制与后续工作

<a id="known-limitations-and-deferred-work"></a>

- **每个工作区只能有一个可写进程**——原生独占锁意味着第二个 DSH 进程无法写入同一工作区集合；它会失败而不会共享。
- **无自动对账**——提供方只索引消费者发送的数据；它不会自行扫描工作区或重放历史会话日志。`dsh-retrieval-session-index` 只覆盖实时事件。
- **本地嵌入可选，且首次使用需要网络**——配置 `embedding` 会从 Hugging Face 下载模型到工作区缓存；无网络部署必须预先填充缓存，或使用调用方向量。
- **模型缓存按工作区隔离**——两个项目使用同一模型会各自保留副本。这是有意为之，以确保所有模型文件都留在使用它的项目内。

<a id="dev-note"></a>
### 开发说明

<details>
<summary>维护者工作上下文——点击展开</summary>

无。

</details>
