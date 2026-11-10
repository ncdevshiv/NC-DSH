---
kind: upgrade-guide
description: "随发行版配置文件现在挂载原生工作区检索，每个工作区获得仓库本地的 .dsh/retrieval 集合。"
---

# 随发行版配置文件挂载原生工作区检索

[English](guide.md) | 中文

## 变更

`dsh-base` 现在默认挂载 `@deepseek-ai/dsh-retrieval-zvec`、`@deepseek-ai/dsh-tool-retrieval` 与 `@deepseek-ai/dsh-retrieval-session-index`。模型获得两个新工具：`workspace_search` 搜索项目文档与历史会话事件，`workspace_index` 以稳定 id 添加或替换可搜索文档。实时会话事件会镜像进同一集合，因此搜索无需单独的 session-query 后端即可看到当前历史。

每个工作区获得仓库本地 `.dsh/retrieval` 目录，存放 zvec 集合、原生日志，以及启用嵌入时的嵌入缓存。本仓库的 `.gitignore` 现在忽略 `/.dsh/`；其他仓库和打包部署也必须忽略或排除同一目录。提供方绝不写到工作区根目录之外。

随发行版组合关闭嵌入：词法全文搜索立即可用，无需网络访问，也不下载模型。需要语义或混合排序的部署在 `retrieval-zvec` 行上设置 `vectorDimension` 与 `embedding`，首次使用时会把模型下载到同一工作区目录。在配置之前，语义与混合请求会以 `RETRIEVAL_SEMANTIC_DISABLED` 被拒绝。

zvec 对每个集合持有独占可写锁。指向同一工作区的第二个 DSH 进程打开集合会失败，而不会共享。

## 迁移

1. 更新那些刻意省略检索的自定义 `cordis.yml`；其余无需改动。已经设置 `session-query-sqlite` 的组合保留其精确搜索行为。

2. 如果部署不能创建项目本地数据，禁用这些行：

   ```yaml
   - id: retrieval-zvec
     disabled: true
   - id: tool-retrieval
     disabled: true
   - id: retrieval-session-index
     disabled: true
   ```

3. 在一个对话中确认索引与搜索：用一个文档调用 `workspace_index`，再用其中的词调用 `workspace_search`。工作区出现 `.dsh/retrieval` 目录，回答会给出文档路径。
