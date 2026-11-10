# 工作区检索基准

[English](README.md) | 中文

## 摘要

一条必需基准通道在相同合成语料上比较随发行版的 zvec 提供方与进程内词法及暴力向量参考实现。它报告检索质量比值、每次查询的 token 成本，以及索引、词法、语义、混合与关闭的实测墙钟/CPU 时间。任何用例都不读取仓库、已记录 Session、用户材料或网络服务。

## 目录

- [运行](#run)
- [测量](#measurements)
- [开发说明](#dev-note)

<a id="run"></a>

## 运行

在仓库根目录先运行 `bun run build:bench` 构建基准，然后运行 `bun exec vitest run --config vitest.bench.config.ts benchmarks/workspace-retrieval/workspace-retrieval.bench.ts`。不要与其他构建或基准重叠计时运行。

工作进程从 `benchmarks/.dsh-build/` 以纯 Node 运行，并通过已构建的 `lib/` 入口解析工作区包。它创建私有临时工作区、写入语料，并在运行后删除根目录。

<a id="measurements"></a>

## 测量

[corpus.ts](corpus.ts) 拥有固定语料。2,000 个文档各 80 个词，包含稀有词法词（每个词出现在 25 个文档中，填充文本绝不包含），或一个类别向量，其文本故意不含语义查询词。十条词法查询与八条语义查询覆盖两种模式；每个文档都带稠密向量，因此语义与混合运行使用同一语料。

| 行 | 比较 |
|---|---|
| 词法 | zvec 原生 FTS 与同一批文档上的精确子串扫描 |
| 语义 | zvec FLAT 余弦搜索与同一批向量上的暴力余弦 |
| 混合 | zvec RRF 融合与两个参考列表上的进程内 RRF |
| token | 两侧返回摘要经 `dsh-token-meter/estimate` 计价 |
| 时间 | 索引、逐查询模式与关闭的墙钟及进程 CPU 毫秒 |

质量断言要求 zvec 在基准文件记录的容差内匹配参考召回率、精确率与倒数排名；token 行必须落在记录的比值内。时间预算使用共享 CI 校准。

<a id="dev-note"></a>

## Dev Note

无。
