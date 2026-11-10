---
description: "把实时会话事件镜像到工作区检索集合，使工作区搜索覆盖当前历史。"
kind: "package-reference"
---

# @deepseek-ai/dsh-retrieval-session-index

[English](README.md) | 中文

## 摘要

使用本包让工作区检索集合与实时会话历史保持同步。每个已提交且语义文本非空的会话事件都会以稳定文档 id `session:<id>:<seq>` 写入会话工作区集合。它补充 session-query 服务：排序式会话搜索仍由已配置的 `ctx.sessionQuery` 后端负责，而 `workspace_search` 通过 `ctx.retrieval` 看到相同会话事件。插件卸载前，处置流程会排空每会话写入链。

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

在检索提供方、会话服务与工作目录服务之后挂载它。随发行版的 `dsh-base` 组合正是如此。

### 配置

| 字段 | 默认值 | 含义 |
|---|---|---|
| `maxTextBytes` | `8192` | 单个事件保留的最大语义文本字节数 |

### 行为

- 只有头部带 `cwd` 的会话会被索引；没有原始项目的会话没有工作区集合。
- 工作区根目录是会话头部的原始 `cwd`，而非当前生效工作目录，因此改变目录的会话仍写入其项目。
- 文档携带 `scope: 'session'`、`sessionId`、`seq`、`type`、`surface` 与 `time`；提供方的元数据过滤可据此缩小范围。
- 空语义文本会被跳过，包括 turn/step 边界和未知的可合并扩展事件类型。
- 每会话写入串行；失败会记录日志，但不会让已提交的会话追加失败。

### 失败

索引失败会带会话 id 记录日志并继续；已提交的会话追加绝不回滚。参数在加载时校验：`maxTextBytes` 必须是正安全整数。

-----

<a id="understand-the-implementation"></a>
## 理解实现

<details>
<summary>实现内部细节——点击展开</summary>

- **一个事件一个文档。** 稳定 id 让重复索引幂等，并保证只有 seq 相同的后续事件才能替换先前事件。
- **原始项目锚点。** 写入根目录来自会话头部的 `cwd`；当前生效工作目录绝不用于存储。
- **每会话写入链。** 每个会话拥有一条承诺尾链，事件按日志顺序追加而不阻塞会话追加。
- **处置至静止。** 插件 effect 在提供方关闭前等待每个实时会话的写入链。

### 源码地图

| 文件 | 角色 |
|---|---|
| [`src/index.ts`](src/index.ts) | 事件订阅、文档投影、工作区锚定、写入链与处置排空 |

</details>

-----

<a id="further-exploration"></a>
## 进一步探索

- [dsh-retrieval](../retrieval/README.zh.md)——本消费者写入的服务。
- [dsh-retrieval-zvec](../retrieval-zvec/README.zh.md)——随发行版提供的提供方。
- [dsh-tool-retrieval](../tool-retrieval/README.zh.md)——面向模型的搜索工具。

-----

<a id="model-experience"></a>
## 模型体验

无，因为 `dsh-retrieval-session-index` 不注册任何工具、提示段落或面向模型的结果；它只把已提交会话文本写入检索集合，之后由 `dsh-tool-retrieval` 渲染。

#### KV 缓存影响

不直接导致失效；请求前缀变化由工具消费者自行负责。

## 已知限制与后续工作

<a id="known-limitations-and-deferred-work"></a>

- **仅实时事件**——启动时不会重放历史日志。集合为空时打开的会话会缺少此前事件，直到其他消费者索引它们。
- **不与持久历史去重**——若另一消费者用不同 id 索引同一事件，会重复出现。
- **文本会被截断**——超过 `maxTextBytes` 的事件只索引前缀，因此该事件后面的词不可搜索。

<a id="dev-note"></a>
### 开发说明

<details>
<summary>维护者工作上下文——点击展开</summary>

无。

</details>
