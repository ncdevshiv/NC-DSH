---
description: "ctx.fs 的 nc-tools 后端文件系统提供方，适用于希望用 nc-tools 的类型化文件语义支撑 harness 自带文件工具的部署。"
kind: "package-reference"
---

# @deepseek-ai/dsh-fs-mcp

[English](README.md) | 中文

## Summary

用 `dsh-fs-mcp` 让 harness 的文件系统工具由 nc-tools MCP server 支撑。该提供方实现 `ctx.fs` 接口，因此每个面向模型的文件工具（`read`、`write`、`edit`、`read_image`）都保留自己的 schema、提示词和策略，而实现换成 nc-tools 的：出现次数精确的编辑会拒绝歧义匹配、写入是原子的、新鲜度守卫基于内容哈希、二进制读取逐字节精确，且错误词汇是模型可据以分支的类型化错误。当文件系统由 harness 不应直接触达的进程提供，或当您希望 nc-tools 那套可记入日志、可重放的文件操作进入会话日志时，选择这个后端。

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

当一个组合应当由 nc-tools MCP server 而非宿主文件系统来提供 `ctx.fs` 时，挂载这个后端。每个会话启动一个 `nc-tools-mcp` 进程并以 MCP stdio 与之通信；每一次读取、写入、编辑和列举都是一次 `tools/call` 往返。

### When to choose it

当文件系统应由 nc-tools 提供时选择 `fs-mcp`——一个挂载的 MCP server、一个共享 server，或 harness 本可自行实现的那套类型化文件语义。当需要直接访问宿主且不愿有进程跳转时选择 [`fs-local`](../fs-local/README.zh.md)；当会话的写入必须限制在其工作区与临时目录内时选择 [`fs-sandbox`](../fs-sandbox/README.zh.md)。

### Minimal configuration

该提供方需要可执行文件以及它所服务的工作区根目录。当会话没有提供自己的工作目录时，工作区根目录就是相对路径的基准；会话自己的目录总是优先，因为工具在每次 resolve 时都会带上它。

```yaml
- name: '@deepseek-ai/dsh-fs-mcp'
  config:
    command: nc-tools-mcp
    workspaceRoot: /absolute/path/to/workspace
```

| Field | Default | Meaning |
|---|---|---|
| `command` | (required) | 要运行的 `nc-tools-mcp` 可执行文件 |
| `args` | `[]` | 可执行文件的参数；工作区根目录会被追加 |
| `workspaceRoot` | `process.cwd()` | 相对路径的基准目录 |
| `toolCallTimeoutMs` | `60000` | 一次 MCP 工具调用的时限 |
| `watchPollMs` | `500` | `watch()` 轮询服务器查变的间隔 |

### What you can do

读取整个文本文件、按窗口流式读取、按您选定的上限读取原始字节、读取一个字节窗口，以及列举一层目录。创建或替换文件，并应用字面量编辑：文本不存在时编辑失败 `FS_EDIT_NOT_FOUND`，匹配多处时编辑失败 `FS_AMBIGUOUS_EDIT`，针对已变更内容规划的编辑失败 `FS_STALE_VERSION`。每一次失败都是带稳定代码的类型化 `FsError`，因此调用方依据代码分支，绝不解析消息文本。

文件的版本标记就是它的内容摘要——与 nc-tools 自身的写入和编辑路径所报告的摘要相同——所以一侧铸造的守卫另一侧也能校验。目录的版本是它的修改时间。

-----

<a id="understand-the-implementation"></a>
## Understand the implementation

### The mapping

每个接口操作都变成一次 nc-tools 工具调用：

| Seam operation | nc-tools call |
|---|---|
| `stat` | `fs.stat`，文件再跟一次 `fs.hash` 取内容摘要 |
| `lstat` | `fs.stat` |
| `readText`, `streamText` | `fs.readRange`，按 `nextByteOffset` 分页 |
| `readBytes`, `readByteRange` | `fs.readBytes`（base64——逐字节精确的窗口） |
| `listDir` | `fs.list` |
| `writeText` | `fs.write`，前置版本校验 |
| `editText` | 先 `patch.check` 取出现次数，再 `patch.apply` |
| `watch` | 每 `watchPollMs` 轮询一次 `fs.stat` |

### Why the version token is a content digest

本地后端从文件系统元数据派生版本号——设备号、inode、大小、纳秒 mtime、纳秒 ctime。nc-tools 的 `fs.stat` 只报告毫秒且没有 inode，那样的元数据标记会漏掉"大小不变、且落在同一毫秒内"的改写。内容摘要没有这两种弱点：它是精确的，它就是 nc-tools 的 `patch.apply` 作为 `previousHash` 返回的东西，代价是一次流式读取。

### Why an edit counts occurrences first

`patch.apply` 要求显式的 `expectedCount`，并且拒绝文件里没有的那个次数。因此提供方先用 `patch.check` 问出该字面量出现多少次，再用那个次数。出现一次就编辑它；出现多次就编辑每一次，这正是 `replaceAll` 的含义。提供方自己拒绝针对歧义匹配的单次编辑，所以失败是 `FS_AMBIGUOUS_EDIT` 而不是服务器错误。

-----

<a id="further-exploration"></a>
## Further Exploration

- [文件系统能力接口](../fs/README.zh.md)——该提供方实现的契约
- [`dsh-fs-local`](../fs-local/README.zh.md)——宿主文件系统后端
- [`dsh-fs-sandbox`](../fs-sandbox/README.zh.md)——带限制的后端
- [`dsh-tool-fs`](../tool-fs/README.zh.md)——该提供方支撑的面向模型的工具
- [nc-tools 协议](https://github.com/ncdevshiv/nc-tools)——服务器的工具面与错误分类

-----

<a id="model-experience"></a>
## Model Experience

### Filesystem tool results

#### What the model sees

相同的文件工具 schema、相同的结果、相同的错误代码：位于接口之后的提供方改变的是字节从何处而来，而不是模型被告知了什么。模型能观察到的差别在失败上——出现多次的文本上的编辑失败 `FS_AMBIGUOUS_EDIT` 而不是替换第一处匹配，针对此后已变更内容规划的编辑失败 `FS_STALE_VERSION` 而不是应用到模型从未见过的文本上。

#### Token effect

不变。该提供方改变的是字节从何处而来，不是模型接收多少字节；工具层自己的上限仍然约束保留的结果。

#### KV Cache effect

没有直接失效；具名的消费方负责任何请求前缀的变化。

-----

<a id="known-limitations-and-deferred-work"></a>
## Known Limitations and Deferred Work

- **每次操作都跨进程。** 一次读取至少是两次调用——一次取版本的 stat，再是分窗的读取——每次都是一次 MCP 往返。SSH 后端付同样的代价；本地后端一分不付。
- **该提供方无法报告符号链接。** nc-tools 的 `fs.stat` 会跟随最后一个路径分量，因此 `lstat` 报告的是链接的**目标**类型。需要在接受前拒绝仓库自有链接的消费方应当使用本地后端。
- **版本标记是内容摘要，所以 `stat` 会读取文件。** 对大文件做 stat 的代价是流式读完该文件。本地后端的元数据标记只需一次系统调用。
- **监视靠轮询。** 监视器按间隔向服务器询问文件元数据；它不是操作系统事件订阅，所以变更在 `watchPollMs` 内被看到，而不是立刻。
- **路径模型由服务器决定。** nc-tools 以自己的基准解析相对路径，并且默认接受任何位置的绝对路径，所以该提供方不是隔离边界。需要隔离时请使用更严格的 `ctx.fs` 后端，或在 `tools/execute` waterfall 上加权限插件。
- **`writeText` 会读取先前内容**以报告接口要求的差异基准，因此一次大覆盖会缓冲旧文本。

<a id="dev-note"></a>
### Dev Note

该提供方是 `ctx.fs` 的一个 Service Provider，也就是同一接口的若干后端之一。它本身不带任何面向模型的工具：工具位于 `dsh-tool-fs` 并针对接口编写，所以替换提供方是一次配置变更，而不是工具变更。

客户端、错误映射和结果形状各自分模块，因而各有唯一归属：`client.ts` 拥有 MCP 连接与 JSON 信封，`errors.ts` 拥有从 nc-tools 的 45 个错误代码到接口的 13 个的翻译，`index.ts` 拥有从接口操作到服务器工具的映射。
