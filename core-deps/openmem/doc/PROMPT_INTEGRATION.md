# Prompt Integration Guide

> How to make any AI agent actually *use* OpenMem — via MCP (preferred) or
> a copy-paste system prompt.

---

## Option A: MCP (preferred)

If your agent supports the [Model Context Protocol](https://modelcontextprotocol.io)
(Claude Code, Cursor, and many others), connect OpenMem once and its tools
appear natively:

```json
{
  "mcpServers": {
    "openmem": {
      "command": "<path-to-python>",
      "args": ["-m", "mcp_server"],
      "env": {}
    }
  }
}
```

Full instructions, per-client details, and troubleshooting:
[`mcp_integration.md`](mcp_integration.md).

The tools need no prompting to work — but adding one line to your agent's
system prompt makes it *proactive* about memory:

```markdown
## Memory
You have persistent memory via the OpenMem MCP tools.
- Before answering questions about the user, their projects, or past work,
  call `recall` with relevant terms.
- When the user shares a durable fact (name, stack, preference, project),
  call `remember` with importance ≥ 0.6.
- For tasks resembling past ones, call `context` first and build on what worked.
```

## Option B: File-based agents

For agents without MCP support, install the skill files and inject context:

```bash
python main.py --skill <agent>     # e.g. claude_code | cursor | generic
python main.py run-cycle           # keep memory fresh
```

Each adapter writes an OpenMem section into the agent's context file
(`CLAUDE.md`, `.cursor/rules/memory.md`, `.qwen/memory_context.md`, ...).
Then add this to the agent's system prompt or rules file:

```markdown
## Your Memory System

You have persistent memory via OpenMem. The "OpenMem Memory Context" section
of your context contains retrieved memories ranked by relevance.

1. Consult it before answering questions about the user, their projects,
   preferences, or previous sessions.
2. Build on recorded lessons; don't repeat approaches marked as failed.
3. New durable facts will be captured automatically by the learning cycle.
```

## Which option?

| Situation | Use |
|---|---|
| Agent speaks MCP | **Option A** — native tools, no file injection, works across projects |
| File-based agent (or minimal setup) | **Option B** — skill files + context injection |
| Both available | A for tools, B's prompt line for proactivity |

## Historical Note

Earlier revisions documented a `/learn` slash-command interface under the
LanceMem brand. That interface no longer exists; the current surfaces are the
MCP tools above, the per-agent triggers (`/mem`, `@memory`, `/lm`), and the
`python main.py` CLI.
