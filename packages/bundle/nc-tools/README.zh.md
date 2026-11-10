---
description: "文件系统工具由 nc-tools MCP server 支撑的 dsh bundle，适用于希望用 nc-tools 的类型化文件操作承载 harness 自带工具的部署。"
kind: "package-bundle"
---

# @deepseek-ai/dsh-nc-tools

[English](README.md) | 中文

## Summary

`dsh-nc-tools` 把 harness 的文件系统实现换成 nc-tools MCP server。模型的文件工具保留自己的名称、schema 和提示词——`read`、`write`、`edit`、`read_image`——而它们的行为变成 nc-tools 的：匹配两次的编辑被拒绝而不是应用到第一处匹配，写入是原子的，过期的编辑被拒绝，二进制读取返回精确字节。这个 bundle 只改动组合中的一行——`ctx.fs` 提供方——所以这次替换是部署方做出的配置选择，而不是一次 fork。

## Table of Contents

- [Use this package](#use-this-package)
- [Understand the implementation](#understand-the-implementation)
- [Further Exploration](#further-exploration)
- [Model Experience](#model-experience)
- [Known Limitations and Deferred Work](#known-limitations-and-deferred-work)
- [Dev Note](#dev-note)

-----

<a id="use-this-package"></a>
## Use this package

把这个 bundle 叠加在 `dsh-base` 之上，让文件系统由 nc-tools 提供。server 必须在同一台机器上：`install-bin` 会把它部署为 `nc-tools-mcp`，`NCTOOLS_MCP_EXE` 指向别的路径。

```bash
dsh --profile nc-tools
```

### What it replaces

一行：`ctx.fs` 提供方。每个面向模型的文件工具都针对该接口编写，所以这次替换改变的是实现，别的一概不变——没有工具 schema，没有提示词段落，没有策略插件，没有会话事件。

### What it does not replace

`glob` 和 `grep` 仍运行打包的 ripgrep 二进制。它们是由进程支撑的发现能力，刻意不扩展 `ctx.fs` 提供方契约，所以没有任何文件系统后端能提供它们。shell 工具（`bash`、`pwsh`）同样保留：nc-tools 按设计没有 shell——它的进程面是带类型化 argv 的 `proc.spawn`，没有命令字符串。

### Configuration

这个 bundle 读取两个环境变量：

| Variable | Default | Meaning |
|---|---|---|
| `NCTOOLS_MCP_EXE` | `nc-tools-mcp` | nc-tools server 可执行文件 |

server 所服务的工作区根目录默认为进程的工作目录。会话自己的工作目录始终优先于它，因为工具在每次 resolve 时都会带上那个目录。

-----

<a id="understand-the-implementation"></a>
## Understand the implementation

### The patch surface over base

这个 bundle 把 `dsh-base` 的 `fs-sandbox` 行替换为 [`dsh-fs-mcp`](../../fs/fs-mcp/README.zh.md)。基础组合的其余部分一概不变：同一套沙箱策略、同一套审批服务、同一套先读后改策略、同一批工具。

### Why a provider swap is the right seam

`ctx.fs` 是单服务接口：每个组合一个实现，由加载哪个插件决定。SSH 后端就是外部实现者的先例，SSH helper 用同样的设计服务远端主机。nc-tools 是本地进程，所以传输是 stdio 上的 MCP 而不是 SSH。

### What the model gains

模型看到的工具不变。变的是它们的行为：出现两次的文本上的编辑现在失败 `FS_AMBIGUOUS_EDIT` 而不是替换第一处匹配，写入不会留下半写的文件，过期的编辑以 `FS_STALE_VERSION` 被拒绝而不是应用到模型从未见过的内容上。

-----

<a id="further-exploration"></a>
## Further Exploration

- [`dsh-fs-mcp`](../../fs/fs-mcp/README.zh.md)——这个 bundle 挂载的提供方
- [文件系统能力接口](../../fs/README.zh.md)——替换背后的契约
- [`dsh-tool-fs`](../../fs/tool-fs/README.zh.md)——保持不变的那些面向模型的工具
- [nc-tools 协议](https://github.com/ncdevshiv/nc-tools)——服务器的工具面与错误分类

-----

<a id="model-experience"></a>
## Model Experience

### Filesystem tool results

#### What the model sees

相同的文件工具，相同的 schema，相同的结果形状——[`read`](../../../docs/tool-catalog.zh.md#deepseek-aidsh-tool-fs)、[`write`](../../../docs/tool-catalog.zh.md#deepseek-aidsh-tool-fs) 和 [`edit`](../../../docs/tool-catalog.zh.md#deepseek-aidsh-tool-fs) 都不变。这个 bundle 改变的是文件系统实现，所以模型的提示词和 schema 都不动；变的是工具在背后做什么——歧义的编辑和过期的编辑被拒绝而不是被应用。

#### Token effect

不变。该提供方改变的是字节从何处而来，不是模型接收多少字节。

#### KV Cache effect

没有直接失效；具名的消费方负责任何请求前缀的变化。

-----

<a id="known-limitations-and-deferred-work"></a>
## Known Limitations and Deferred Work

- **server 是硬依赖。** 加载这个 bundle 却无法连通 `nc-tools-mcp` 的组合无法提供文件系统，会话的文件工具会以传输错误而非文件错误失败。
- **每次文件系统操作都跨进程**，因此一次读取至少两次 MCP 往返。本地后端一分不付。
- **该提供方无法报告符号链接**，因此需要在解析前拒绝仓库自有链接的消费方在这里得不到该保证。
- **nc-tools 的路径模型不是隔离边界**：相对路径以所服务的工作区解析，绝对路径在任何位置都被接受。当需求是隔离时请使用更严格的 `ctx.fs` 后端。
- **搜索与 shell 不属于 nc-tools**：`glob` 和 `grep` 仍由 ripgrep 支撑，shell 工具仍是 harness 自己的。

<a id="dev-note"></a>
### Dev Note

这个 bundle 是一个 patch 层，不是一个插件：它只携带一行，不含代码。`dsh-base` 保留自己的 `fs-sandbox` 行，所以把此 bundle 叠加在 base 之上会替换那一行，而不是新增第二个提供方——一个组合里两个 `ctx.fs` 提供方是重复服务，Cordis 会拒绝。
