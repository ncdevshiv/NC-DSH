---
description: "工作区检索包组映射：zvec 驱动的词法、语义与混合搜索服务，面向模型工具以及实时会话事件索引器。"
kind: "package-group"
---

# retrieval/ —— 工作区检索能力族

[English](README.md) | 中文

## 摘要

`retrieval/` 包组为项目提供自己的排序搜索能力，覆盖文件与会话文本。一个服务契约定义词法、语义与混合检索；一个提供方用原生 zvec 引擎实现它，并把每个集合、日志与模型缓存都存进工作区；一对面向模型的工具暴露搜索与索引；一个消费者把实时会话事件镜像到同一集合。词法搜索无需嵌入模型即可工作，因此默认启用检索不需要网络或模型下载。本页负责包组映射；每个包的 README 拥有其包级契约。

## 目录

- [包](#packages)
- [相关文档](#related-documentation)
- [开发说明](#dev-note)

-----

<a id="packages"></a>
## 包

| 包 | 角色 | ctx 键 |
|---|---|---|
| [`retrieval/`](retrieval/README.zh.md) | 工作区检索服务定义：记录、模式、过滤、错误 | `ctx.retrieval` |
| [`retrieval-zvec/`](retrieval-zvec/README.zh.md) | 原生 zvec 提供方：仓库本地集合、FTS、稠密向量、可选本地嵌入 | 注册 `ctx.retrieval` |
| [`retrieval-session-index/`](retrieval-session-index/README.zh.md) | 把已提交的实时会话事件镜像进工作区集合 | — |
| [`tool-retrieval/`](tool-retrieval/README.zh.md) | 面向模型的 `workspace_search` 与 `workspace_index` 工具 | `ctx.tools` |

-----

<a id="related-documentation"></a>
## 相关文档

- [检索子系统](../../docs/subsystems/retrieval.zh.md)——类型级契约、模式与仓库本地存储规则。
- [会话查询能力族](../session-query/README.zh.md)——精确历史读取与 SQLite 全文搜索后端。
- [存储子系统](../../docs/subsystems/storage.zh.md)——独立的非会话 KV 持久化接缝。
- [zvec 上游](https://github.com/alibaba/zvec)——随发行版提供方背后的引擎。

`dsh-base` 组合默认挂载 `retrieval-zvec`、`tool-retrieval` 与 `retrieval-session-index`（关闭嵌入），因此随发行版配置文件无需下载模型即可获得原生工作区全文搜索。

-----

<a id="dev-note"></a>
## 开发说明

<details>
<summary>维护者工作上下文——点击展开</summary>

无。

</details>
