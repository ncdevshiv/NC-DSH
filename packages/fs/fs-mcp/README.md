---
description: "The nc-tools-backed filesystem provider for ctx.fs, for deployments that want nc-tools' typed file semantics behind the harness's own file tools."
kind: "package-reference"
---

# @deepseek-ai/dsh-fs-mcp

English | [中文](README.zh.md)

## Summary

Use `dsh-fs-mcp` to back the harness's filesystem tools with the nc-tools MCP server. The provider implements the `ctx.fs` seam, so every model-facing file tool (`read`, `write`, `edit`, `read_image`) keeps its schema, its prompt, and its policy while its implementation becomes nc-tools': occurrence-exact edits that refuse an ambiguous match, atomic writes, a content-hash freshness guard, byte-exact binary reads, and a typed error vocabulary the model can branch on. Choose this backend when the filesystem is served by a process you do not want the harness to reach directly, or when you want nc-tools' journaled, replayable file operations in the session log.

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

Mount this backend when a composition should serve `ctx.fs` from an nc-tools MCP server instead of the host filesystem. The provider starts one `nc-tools-mcp` process per session and speaks MCP stdio to it; every read, write, edit, and listing is one `tools/call` round trip.

### When to choose it

Choose `fs-mcp` when the filesystem is nc-tools' to serve — a mounted MCP server, a shared server, or the same typed file semantics the harness would otherwise have to reimplement. Choose [`fs-local`](../fs-local/README.md) for direct host access with no process hop, and [`fs-sandbox`](../fs-sandbox/README.md) when a session's writes must be confined to its workspace.

### Minimal configuration

The provider needs the executable and the workspace root it serves. The workspace root is the base for relative paths when a session supplies no working directory of its own; a session's own directory always wins, because the tools pass it with every resolve.

```yaml
- name: '@deepseek-ai/dsh-fs-mcp'
  config:
    command: nc-tools-mcp
    workspaceRoot: /absolute/path/to/workspace
```

| Field | Default | Meaning |
|---|---|---|
| `command` | (required) | The `nc-tools-mcp` executable to run |
| `args` | `[]` | Arguments for the executable; the workspace root is appended |
| `workspaceRoot` | `process.cwd()` | Base directory for relative paths |
| `toolCallTimeoutMs` | `60000` | Budget for one MCP tool call |
| `watchPollMs` | `500` | How often `watch()` polls the server for a change |

### What you can do

Read a whole text file, stream it in windows, read raw bytes up to a cap you choose, read one byte window, and list one directory level. Create or replace a file, and apply a literal edit: an edit whose text is absent fails `FS_EDIT_NOT_FOUND`, one that matches more than once fails `FS_AMBIGUOUS_EDIT`, and one planned against content that has since changed fails `FS_STALE_VERSION`. Every failure is a typed `FsError` with a stable code, so a consumer branches on the code and never on message text.

The version token of a file is its content digest — the same digest nc-tools' own write and edit paths report — so a guard minted from one side is checkable by the other. A directory's version is its modification time.

-----

<a id="understand-the-implementation"></a>
## Understand the implementation

### The mapping

Each seam operation becomes one nc-tools tool call:

| Seam operation | nc-tools call |
|---|---|
| `stat` | `fs.stat`, then `fs.hash` for a file's content digest |
| `lstat` | `fs.stat` |
| `readText`, `streamText` | `fs.readRange`, paged by `nextByteOffset` |
| `readBytes`, `readByteRange` | `fs.readBytes` (base64 — the byte-exact window) |
| `listDir` | `fs.list` |
| `writeText` | `fs.write`, preceded by the version check |
| `editText` | `patch.check` for the occurrence count, then `patch.apply` |
| `watch` | `fs.stat` polled on `watchPollMs` |

### Why the version token is a content digest

The local backend derives a version from filesystem metadata — device, inode, size, nanosecond mtime, nanosecond ctime. nc-tools' `fs.stat` reports milliseconds and no inode, so a metadata token there would miss a rewrite that kept the size and landed in the same millisecond. The content digest has neither weakness: it is exact, it is what nc-tools' `patch.apply` returns as `previousHash`, and it costs one streamed read.

### Why an edit counts occurrences first

`patch.apply` requires an explicit `expectedCount`, and it refuses a count the file does not have. The provider therefore asks `patch.check` how many times the literal occurs, and uses that count. One occurrence edits it; more than one edits every occurrence, which is what `replaceAll` means. The provider refuses a single-occurrence edit against an ambiguous match itself, so the failure is `FS_AMBIGUOUS_EDIT` rather than a server error.

-----

<a id="further-exploration"></a>
## Further Exploration

- [The filesystem capability seam](../fs/README.md) — the contract this provider implements
- [`dsh-fs-local`](../fs-local/README.md) — the host-filesystem backend
- [`dsh-fs-sandbox`](../fs-sandbox/README.md) — the confining backend
- [`dsh-tool-fs`](../tool-fs/README.md) — the model-facing tools this provider backs
- [The nc-tools protocol](https://github.com/ncdevshiv/nc-tools) — the server's tool surface and error taxonomy

-----

<a id="model-experience"></a>
## Model Experience

### Filesystem tool results

#### What the model sees

The same file tool schemas, the same results, and the same error codes the local backend produces: a provider behind the seam changes where the bytes come from, not what the model is told. A difference the model can observe is in the failures — an edit whose text occurs twice fails `FS_AMBIGUOUS_EDIT` rather than replacing the first match, and an edit planned against content that has since changed fails `FS_STALE_VERSION` rather than applying to text the model never saw.

#### Token effect

Unchanged. The provider changes where the bytes come from, not how many the model receives; the tool layer's own caps still bound the retained result.

#### KV Cache effect

No direct invalidation; the named consumer owns any request-prefix changes.

## Known Limitations and Deferred Work

<a id="known-limitations-and-deferred-work"></a>

- **Every operation crosses a process boundary.** A read is at least two calls — a stat for the version, then the windowed read — and each is one MCP round trip. The SSH-backed provider pays the same cost; the local backend pays none.
- **The provider cannot report a symlink.** nc-tools' `fs.stat` follows the final path component, so `lstat` reports a link's TARGET type. A consumer that must reject repository-owned links before resolving them needs the local backend.
- **The version token is a content digest, so `stat` reads the file.** A stat of a large file costs a streamed read of that file. The local backend's metadata token costs a syscall.
- **Watching polls.** The watcher asks the server for the file's metadata on an interval; it is not an OS event subscription, so a change is seen within `watchPollMs` rather than immediately.
- **The server owns the path model.** nc-tools resolves relative paths against its own base and accepts absolute paths anywhere by default, so this provider is not a containment boundary. Enforce confinement with a stricter `ctx.fs` backend or a permission plugin on the `tools/execute` waterfall.
- **A `writeText` reads the prior content** to report the diff basis the seam asks for, so a large overwrite buffers the previous text.

<a id="dev-note"></a>
### Dev Note

The provider is a `ctx.fs` Service Provider, which means it is one of several backends for one seam. It carries no model-facing tool of its own: the tools live in `dsh-tool-fs` and are written against the seam, so swapping the provider is a configuration change rather than a tool change.

The client, the error mapping, and the result shapes are separate modules so each has one owner: `client.ts` owns the MCP connection and the JSON envelope, `errors.ts` owns the translation from nc-tools' 45 error codes to the seam's 13, and `index.ts` owns the mapping from the seam's operations to the server's tools.
