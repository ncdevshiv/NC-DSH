---
description: "面向模型的工作区搜索与索引工具，基于工作区检索服务。"
kind: "package-reference"
---

# @deepseek-ai/dsh-tool-retrieval

[English](README.md) | 中文

## 摘要

`dsh-tool-retrieval` 在 `ctx.retrieval` 上注册两个面向模型的工具：`workspace_search` 按文本或语义查找项目文件与历史会话事件，`workspace_index` 以稳定 id 添加或替换可搜索文档。二者都通过调用方的原始项目目录限定工作区，因此改过工作目录的会话仍然读写其仓库拥有的集合。工具返回有界纯文本，并使用提供方自身的词法、语义或混合排序。

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

在检索提供方与工作目录服务之后挂载它。随发行版的 `dsh-base` 组合将其与 `dsh-retrieval-zvec`、`dsh-retrieval-session-index` 一并挂载。

### 工具

| 工具 | 参数 | 行为 |
|---|---|---|
| `workspace_search` | `query`、`mode?`、`path?`、`session_id?`、`event_types?`、`event_surfaces?`、`time_from?`、`time_to?` | 搜索调用方工作区，返回带位置、范围、分数和有界摘要的排序命中。`mode` 默认为 `lexical`；`semantic` 与 `hybrid` 需要支持嵌入的部署 |
| `workspace_index` | `documents[]` | 按稳定 id 写入文档；每个文档包含文本，以及可选的路径、会话元数据、时间和调用方向量 |

`workspace_search` 使用本包配置的最大结果数，且返回不超过该页。最后一页没有续页提示；更早的页面会告知模型还有更多匹配，并应缩小查询范围。

### 配置

| 字段 | 默认值 | 含义 |
|---|---|---|
| `maxResults` | `20` | 单次搜索返回的最大命中数 |
| `timeoutMs` | `30000` | 任一工具的协作式截止时间 |

### 失败

非法参数在调用提供方之前以 `RETRIEVAL_TOOL_INVALID_ARGUMENTS` 失败。缺少绑定 agent 的调用方以 `RETRIEVAL_TOOL_MISSING_AGENT` 失败。提供方失败会记录日志并转换为 `RETRIEVAL_TOOL_FAILED`，避免内部路径和原生错误文本进入模型。

-----

<a id="understand-the-implementation"></a>
## 理解实现

<details>
<summary>实现内部细节——点击展开</summary>

- **锚定原始项目。** 工具读取 `agent.session.header.cwd`，而非当前生效工作目录，因此 `cd` 之后索引与搜索数据仍留在仓库内。
- **与提供方无关。** 工具只调用 `ctx.retrieval`；它们不认识 zvec、SQLite 或其他介质。
- **有界输出。** 每个命中贡献简短位置、分数和摘要；完整结果不超过 `maxResults`。

### 源码地图

| 文件 | 角色 |
|---|---|
| [`src/index.ts`](src/index.ts) | 工具 schema、参数规范化、工作区锚定、检索调用与文本渲染 |

</details>

-----

<a id="further-exploration"></a>
## 进一步探索

- [dsh-retrieval](../retrieval/README.zh.md)——这些工具消费的服务。
- [dsh-retrieval-zvec](../retrieval-zvec/README.zh.md)——随发行版提供的提供方。
- [dsh-retrieval-session-index](../retrieval-session-index/README.zh.md)——实时会话事件索引器。

-----

<a id="model-experience"></a>
## 模型体验

### 系统提示

#### 模型看到什么

`tool:retrieval` 段落说明何时使用各工具、词法模式按文本精确匹配，以及语义和混合模式需要支持嵌入的部署。

##### 检索指引

```markdown
Use workspace_search to find relevant project files and prior session events. Lexical mode matches text exactly; semantic and hybrid modes rank by meaning and require an embedding-capable deployment. Results are workspace-scoped and each hit names its source.
Use workspace_index to add or refresh searchable workspace documents. Pass stable ids; re-indexing the same id replaces it.
```

#### Token 影响

在段落可见的每次请求上产生固定指引成本。

#### KV 缓存影响

只要段落与工具可见性不变，前缀保持稳定。工具限制、激活或处置可能从第一个变化的段落开始使复用失效。

### 工具 schema

#### 模型看到什么

生成的 [`workspace_search` 与 `workspace_index` schema](../../../docs/tool-catalog.zh.md#deepseek-aidsh-tool-retrieval) 描述其参数与默认模式。结果是纯文本，不是结构化 JSON。

#### Token 影响

工具可见时为固定 schema 成本；每次结果增加有界保留 token。

#### KV 缓存影响

只要工具可见性与定义不变，前缀保持稳定。

### 结果

#### 模型看到什么

`Workspace matches (N):` 后跟带位置、范围、分数和摘要的编号命中；空页为 `No workspace matches found.`；成功索引为 `Indexed N workspace documents.`。

#### Token 影响

结果受 `maxResults` 限制，每个摘要最多 240 个 Unicode 码点；保留的调用与结果在压缩前留在历史中。

#### KV 缓存影响

追加式；新可见内容位于可复用请求前缀之后，不会使已有 KV 缓存条目失效。

### 工具错误

#### 模型看到什么

参数失败为 `Error: <message>`，并带有结构化的 `RETRIEVAL_TOOL_INVALID_ARGUMENTS`、`RETRIEVAL_TOOL_MISSING_AGENT` 或 `RETRIEVAL_TOOL_FAILED` 元数据。

#### Token 影响

只有失败调用会增加这些保留 token。

#### KV 缓存影响

追加式。

## 已知限制与后续工作

<a id="known-limitations-and-deferred-work"></a>

- **不自动索引工作区文件**——`workspace_index` 是显式操作；工具本身不扫描仓库，也不监视变更。
- **没有续页后续调用**——非末页只会提示模型缩小范围，不暴露游标，因此一次调用最多返回 `maxResults` 个命中。
- **语义模式取决于部署**——仅词法部署会拒绝语义与混合请求，而不是静默回退为文本。

<a id="dev-note"></a>
### 开发说明

<details>
<summary>维护者工作上下文——点击展开</summary>

无。

</details>
