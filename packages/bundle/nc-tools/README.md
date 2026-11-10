---
description: "The dsh bundle whose filesystem tools are backed by the nc-tools MCP server, for deployments that want nc-tools' typed file operations behind the harness's own tools."
kind: "package-bundle"
---

# @deepseek-ai/dsh-nc-tools

English | [中文](README.zh.md)

## Summary

`dsh-nc-tools` swaps the harness's filesystem implementation for the nc-tools MCP server. The model's file tools keep their names, schemas, and prompts — `read`, `write`, `edit`, `read_image` — and their behavior becomes nc-tools': an edit that matches twice is refused rather than applied to the first match, a write is atomic, a stale edit is refused, and a binary read returns the exact bytes. The bundle changes one row of the composition — the `ctx.fs` provider — so the swap is a configuration choice a deployment makes, not a fork.

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

Layer this bundle over `dsh-base` to serve the filesystem from nc-tools. The server must be on the machine: `install-bin` deploys it as `nc-tools-mcp`, and `NCTOOLS_MCP_EXE` points the bundle at a different path.

```bash
dsh --profile nc-tools
```

### What it replaces

One row: the `ctx.fs` provider. Every model-facing filesystem tool is written against that seam, so the swap changes the implementation and nothing else — no tool schema, no prompt section, no policy plugin, no session event.

### What it does not replace

`glob` and `grep` still run the packaged ripgrep binary. They are spawn-backed discovery that deliberately does not extend the `ctx.fs` provider contract, so no filesystem backend can supply them. The shell tools (`bash`, `pwsh`) also stay: nc-tools has no shell by design — its process surface is `proc.spawn` with typed argv, no command string.

### Configuration

The bundle reads two environment variables:

| Variable | Default | Meaning |
|---|---|---|
| `NCTOOLS_MCP_EXE` | `nc-tools-mcp` | The nc-tools server executable |

The workspace root the server serves defaults to the process's working directory. A session's own working directory always wins over it, because the tools pass that directory with every resolve.

-----

<a id="understand-the-implementation"></a>
## Understand the implementation

### The patch surface over base

The bundle replaces the `fs-sandbox` row of `dsh-base` with [`dsh-fs-mcp`](../../fs/fs-mcp/README.md). Nothing else in the base composition changes: the same sandbox policy, the same approval service, the same read-before-edit policy, the same tools.

### Why a provider swap is the right seam

`ctx.fs` is a single-service seam: one implementation per composition, chosen by which plugin is loaded. The SSH-backed provider is the precedent for an external one, and the SSH helper the same design serves a remote host. nc-tools is a local process, so the transport is MCP over stdio rather than SSH.

### What the model gains

The model sees the same tools. What changes is what they do: an edit whose text occurs twice now fails `FS_AMBIGUOUS_EDIT` instead of replacing the first match, a write cannot leave a half-written file, and a stale edit is refused with `FS_STALE_VERSION` rather than applied to content the model never saw.

-----

<a id="further-exploration"></a>
## Further Exploration

- [`dsh-fs-mcp`](../../fs/fs-mcp/README.md) — the provider this bundle mounts
- [The filesystem capability seam](../../fs/README.md) — the contract behind the swap
- [`dsh-tool-fs`](../../fs/tool-fs/README.md) — the model-facing tools that stay the same
- [The nc-tools protocol](https://github.com/ncdevshiv/nc-tools) — the server's tool surface and error taxonomy

-----

<a id="model-experience"></a>
## Model Experience

### Filesystem tool results

#### What the model sees

The same file tools with the same schemas and the same result shapes — [`read`](../../../docs/tool-catalog.md#deepseek-aidsh-tool-fs), [`write`](../../../docs/tool-catalog.md#deepseek-aidsh-tool-fs), and [`edit`](../../../docs/tool-catalog.md#deepseek-aidsh-tool-fs) are unchanged. The bundle changes the filesystem implementation, so the model's prompt and schemas do not move; what changes is what the tools do behind them — an ambiguous edit and a stale edit are refused rather than applied.

#### Token effect

Unchanged. The provider changes where the bytes come from, not how many the model receives.

#### KV Cache effect

No direct invalidation; the named consumer owns any request-prefix changes.

## Known Limitations and Deferred Work

<a id="known-limitations-and-deferred-work"></a>

- **The server is a hard dependency.** A composition that loads this bundle without a reachable `nc-tools-mcp` cannot serve the filesystem, and the session's file tools fail with the transport error rather than a file error.
- **Every filesystem operation crosses a process boundary**, so a read is at least two MCP round trips. The local backend pays none.
- **The provider cannot report a symlink**, so a consumer that must reject repository-owned links before resolving them does not get that guarantee here.
- **The nc-tools path model is not a containment boundary**: relative paths resolve against the served workspace, and absolute paths are accepted anywhere. Use a stricter `ctx.fs` backend when confinement is the requirement.
- **Search and shell are not nc-tools'**: `glob` and `grep` remain ripgrep-backed, and the shell tools remain the harness's own.

<a id="dev-note"></a>
### Dev Note

The bundle is a patch layer, not a plugin: it carries one row and no code. `dsh-base` keeps its own `fs-sandbox` row, so a composition that layers this bundle over base replaces that row rather than adding a second provider — two `ctx.fs` providers in one composition would be a duplicate service, and Cordis refuses it.
