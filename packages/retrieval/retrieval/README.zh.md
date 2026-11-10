---
description: "工作区检索服务契约，覆盖文件与会话事件的词法、语义和混合搜索。"
kind: "package-reference"
---

# @deepseek-ai/dsh-retrieval

[English](README.md) | 中文

## 摘要

`dsh-retrieval` 定义一个按工作区划分的检索服务：消费者把文档写入项目集合，再用词法、语义或混合排序进行搜索。它拥有请求与结果记录、稳定错误码以及 `ctx.retrieval` 服务键；而提供方（例如 [`dsh-retrieval-zvec`](../retrieval-zvec/README.zh.md)）拥有具体介质。当消费者需要对项目内容做排序搜索、又不想绑定某一种存储引擎时使用它。

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

先挂载一个注册 `ctx.retrieval` 的提供方；本包从不单独挂载。随后消费者调用五类操作：

| 操作 | 结果 |
|---|---|
| `indexDocuments(request)` | 按稳定 id 将文档写入一个工作区集合 |
| `removeDocuments(request)` | 按 id 列表或元数据过滤删除文档 |
| `search(request)` | 返回一页排序结果，可选续页游标 |
| `stats()` | 报告已打开的工作区集合数与文档总数 |
| `close()` | 关闭所有集合并释放提供方资源 |

搜索请求指定绝对工作区根目录、查询文本、`lexical`/`semantic`/`hybrid` 模式、分页上限、可选元数据过滤、可选游标以及可选调用方向量。提供方返回 `RetrievalHit[]`；每个命中保留其来源范围以及可选的路径、会话 id、事件序号、类型、表面和时间。查询文本按数据处理，绝不当作可执行的索引语法。

### 过滤

`RetrievalSearchFilter` 对收到的字段做 AND：来源范围、工作区路径、会话 id、事件类型、事件表面以及闭区间事件时间范围。提供方把过滤条件翻译成自己的查询语言；服务会拒绝反向时间区间和非法值，并报 `RETRIEVAL_INVALID_FILTER`。

### 失败

所有失败都是带类型的 `RetrievalError`，错误码为 `RETRIEVAL_ABORTED`、`RETRIEVAL_INVALID_FILTER`、`RETRIEVAL_INVALID_QUERY`、`RETRIEVAL_INVALID_REQUEST`、`RETRIEVAL_PROVIDER_FAILED`、`RETRIEVAL_SEMANTIC_DISABLED` 或 `RETRIEVAL_WORKSPACE_NOT_FOUND`。取消是每次调用传入的 `AbortSignal`；即使提供方无法在取消时立即静止，也必须先完成自身承诺再拒绝。

### 编写提供方

提供方拥有介质与生命周期，必须做到：

- 校验工作区根目录，并把数据建在对应项目内部；
- 只接受已配置的向量维度，并显式拒绝不支持的语义请求；
- 生成对调用方不透明的游标，并绑定到同一规范化请求；
- 保持已接受写入的顺序，并在报告持久化前 await `flush()`；
- 让 `close()` 幂等，并在释放介质前排空已接受的写入。

-----

<a id="understand-the-implementation"></a>
## 理解实现

<details>
<summary>实现内部细节——点击展开</summary>

本节解释契约背后的设计取舍，并指向实现它们的代码。

- **是服务，不是存储。** 本包拥有词汇与 `ctx.retrieval` 接缝；提供方拥有集合、索引、排序和持久化；消费者从不直接访问某引擎。
- **按请求划分工作区。** 每个操作都指定工作区根目录，因此一个提供方实例可服务多个项目，而不依赖进程内的“当前工作区”。
- **语义能力是显式的。** 词法搜索在无嵌入模型时也必须可用；语义和混合排序需要已配置向量或已配置的本地嵌入器。

### 源码地图

| 文件 | 角色 |
|---|---|
| [`src/index.ts`](src/index.ts) | 抽象 `RetrievalService`、请求与结果记录、错误码以及 `ctx.retrieval` 声明 |

</details>

-----

<a id="further-exploration"></a>
## 进一步探索

- [存储子系统](../../../docs/subsystems/storage.zh.md)——相关的 KV 与域持久化接缝。
- [dsh-retrieval-zvec](../retrieval-zvec/README.zh.md)——随发行版提供的 zvec 提供方。
- [dsh-tool-retrieval](../tool-retrieval/README.zh.md)——面向模型的工作区搜索与索引工具。

-----

<a id="model-experience"></a>
## 模型体验

间接地通过 `dsh-tool-retrieval`：由它把这些命中与失败渲染为有界工作区工具结果。

#### KV 缓存影响

不直接导致失效；请求前缀变化由上述消费者自行负责。

## 已知限制与后续工作

<a id="known-limitations-and-deferred-work"></a>

- **单页游标契约**——服务定义不透明字符串游标，但不统一其负载格式；游标有效性与过期处理由提供方负责。
- **无批量事务**——每个操作独立持久化；服务不提供跨工作区事务或回滚。
- **维度由提供方负责**——服务本身不校验向量；提供方必须在写入前拒绝维度不匹配。

<a id="dev-note"></a>
### 开发说明

<details>
<summary>维护者工作上下文——点击展开</summary>

无。

</details>
