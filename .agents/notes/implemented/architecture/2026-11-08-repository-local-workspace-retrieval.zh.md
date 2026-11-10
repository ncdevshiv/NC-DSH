# Agent Note: 通过 zvec 实现仓库本地的工作区检索

Status: implemented

[English](2026-11-08-repository-local-workspace-retrieval.md) | 中文

## 问题

此前的 harness 有两条检索路径，却没有共享的一条。会话历史搜索只是一个随发行版默认关闭（`openAt: never`）的 SQLite FTS 后端；面向模型的文件发现只是词法 ripgrep，无法按语义搜索项目内容。当用户要求 agent 找到相关历史工作或项目文本时，没有默认能力：内容搜索被关闭，`glob`/`grep` 需要精确模式，也没有任何东西索引会话事件以供后续搜索。

需求是集成原生 [zvec](https://github.com/alibaba/zvec) 引擎、默认启用它，并把每一个 zvec 拥有的文件——集合、索引、原生日志和嵌入缓存——都限制在每个项目的仓库本地数据目录内。同时要求提供诚实的含/不含对比测量，覆盖质量、token 成本、延迟与取舍。

## 决定

### 工作区检索接缝，而不是第二个会话后端

`@deepseek-ai/dsh-retrieval` 声明 `ctx.retrieval`：索引、删除、搜索、统计、flush、关闭。每个请求都指定绝对工作区根目录，因此一个提供方实例可服务多个项目，而不依赖进程级“当前工作区”状态。接缝拥有记录、模式（`lexical`、`semantic`、`hybrid`）、元数据过滤、游标与稳定错误码；提供方拥有介质。

它与 `ctx.sessionQuery` 分离：后者保留精确历史读取、过滤、追踪及其 SQLite FTS 排序。替换该后端会迫使精确搜索调用方经过向量排序并改变游标语义；新接缝让检索承载文件与会话文本，而不改动会话历史权威。

### zvec 作为随发行版提供方

`@deepseek-ai/dsh-retrieval-zvec` 用官方 Node 绑定注册 `ctx.retrieval`。每个工作区在 `<root>/<dataDir>/<collectionName>` 拥有一个集合，`dataDir` 默认为 `.dsh`；原生日志文件与 fastembed 模型缓存共享该目录。提供方校验根目录、按 zvec 原生 1024 文档上限切片写入、在同一条承诺链上串行已接受写入、暴露 `flush()`，并在处置时关闭所有集合。

集合始终对 `text` 建立全文索引，并对 `scope`、`path`、`session_id`、`seq`、`type`、`surface` 和 `time` 建立倒排索引。仅当配置了 `vectorDimension` 时才存在向量字段，因此仅词法部署不承担向量内存，也不会静默接受向量。`hybrid` 运行一个 FTS 子查询和一个向量子查询，并以 RRF 排名常数 60 融合。查询文本被引用为一个字面 FTS 短语，因此索引语法保持惰性。

zvec 对每个集合路径持有独占可写锁。第二次可写打开会失败而不会共享，因此同一时间只有一个 DSH 进程拥有某工作区集合；提供方记录该失败，而不是增加静默只读回退。

### 嵌入是可选的，且绝不在启动时下载

zvec 不生成嵌入。随发行版的 `dsh-base` 组合以 `embedding: false` 挂载提供方，因此词法工作区搜索立即可用，无需网络访问，也不下载模型。语义与混合请求会以 `RETRIEVAL_SEMANTIC_DISABLED` 失败，直到部署配置 `vectorDimension` 并要么提供调用方向量、要么启用 `fastembed` 模型——其权重留在 `<workspace>/.dsh/retrieval-models`。已配置的模型与显式维度必须一致，这在加载时检查。

### 一对模型工具与一个实时索引器

`@deepseek-ai/dsh-tool-retrieval` 注册 `workspace_search` 与 `workspace_index`。二者锚定 `agent.session.header.cwd`，而非当前生效工作目录，因此改变目录的 Session 仍读写其仓库的集合。搜索工具隐藏游标、按 `maxResults` 限制命中、渲染短摘要，并把提供方失败转换为 `RETRIEVAL_TOOL_FAILED`。

`@deepseek-ai/dsh-retrieval-session-index` 订阅 `session/event`，把每个具有语义文本的已提交事件投影为 `session:<id>:<seq>` 文档，并写出每会话承诺链，处置时排空。它写入会话头部的原始 `cwd`，跳过无文本事件，并按 `maxTextBytes` 截断文本。

三行以及提供方在 `dsh-base` 中无条件挂载；Web bundle 继承它们。仓库的 `.gitignore` 忽略 `/.dsh/`，升级指南也说明该目录与可关闭的行。

## 考虑过的其他方案

- **用 zvec `SessionQueryEngine` 替换 `session-query-sqlite`。** 否决：精确历史游标世代、对账与摘要语义对 Web 侧边栏和 `tool-session-query` 都是关键的；向量引擎会改变排序与游标含义。检索接缝是增加能力，而不是换掉权威。
- **默认启用本地嵌入。** 否决：首次运行的 Hugging Face 下载会让随发行版默认依赖网络和磁盘，并把模型下载延迟藏在首次搜索里。仅词法默认让激活无副作用，语义部署可显式选择加入。
- **即使不使用也在启动时加载 `fastembed` 模型。** 否决：可选依赖与模型加载成本只应在语义请求真正需要时支付；随发行版组合从不支付任何一项。
- **把集合存放在 Harness home 下。** 因需求否决：项目数据必须留在项目内。代价是跨项目重复的模型缓存，已记为限制。
- **把精确搜索语义照搬进 `workspace_search`。** 否决：`glob`/`grep` 仍是具有自身契约的精确词法工具；`workspace_search` 是排序检索，并在指引中说明。需要字面扫描的调用方继续使用 grep 或 `filterEvents()`。
- **回退到当前生效工作目录进行存储。** 否决：`cd` 之后写入会落到拥有该 Session 的仓库之外；锚定头部可保持仓库身份稳定。

## 后果

工作区检索是真实的默认能力：模型无需配置即可搜索当前会话文本与已索引的项目文档，所有字节都留在工作区的 `.dsh/` 目录树内。精确历史搜索保留其 SQLite 契约；两个搜索表面回答不同问题。

在匹配的合成语料上，zvec 提供方与进程内词法与暴力向量参考完全一致：召回率 0.40（25 个相关文档截断到前 10）、精确率 1.0、倒数排名 1.0，词法、语义与混合在两侧的命中率均为 1.0，两侧每次查询返回摘要的估算 token 均为 640。索引 2,000 个 80 词文档约需 0.4 秒；每次查询中位数约 0.36 毫秒（词法）、0.27 毫秒（语义）、0.19 毫秒（混合），关闭约需 0.41 秒。参考行故意是进程内、无索引实现，因此这些时间比值描述的是该测试夹具，而非产品级速度声明；基准证明的成本是 token 与质量对等，延迟数字作为预算记录，而不是与 SQLite 的直接对比。

写入路径每个文档切片需要一次原生同步批处理，关闭需要一次完整遍历；二者都由基准度量。两条限制是显式的：因原生锁，每个工作区只能有一个可写进程；无自动对账——提供方只索引消费者发送的数据，而消费者目前只镜像实时事件，不包含历史日志。

验证位于检索包的测试套件（提供方生命周期、持久化与重开、过滤、游标分页、带有调用方向量的语义与混合、通过模拟 fastembed 边界的嵌入模型接线、负路径，以及带 `cordis.yml` 树的真实 Loader 组合）、base bundle 的组合断言、headless 配置无密钥冒烟（断言持久化请求头中出现 `workspace_search` 与 `workspace_index`），以及必需的 `workspace-retrieval` 基准通道。

## 相关

- [检索子系统](../../../../docs/subsystems/retrieval.md)
- [会话查询子系统](../../../../docs/subsystems/session-query.md)
- [存储子系统](../../../../docs/subsystems/storage.md)
