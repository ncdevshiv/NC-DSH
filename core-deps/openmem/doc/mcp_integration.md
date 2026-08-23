# OpenMem MCP Integration

OpenMem ships a native [Model Context Protocol](https://modelcontextprotocol.io)
server, so any MCP-capable client can use the memory layer directly — no
skill files, no context-file injection, no wrapper scripts.

- Server module: `mcp_server.py` (repo root)
- Console script: `openmem-mcp` (after `pip install -e .[mcp]`)
- Transport: **stdio**
- SDK: `mcp>=2.0.0` (optional dependency — never a core requirement)

## Tools

| Tool | Signature | Returns |
|---|---|---|
| `remember` | `remember(content: str, importance: float = 0.5, tags: list[str] = [])` | New memory id (str) |
| `recall` | `recall(query: str, limit: int = 5)` | Formatted hits: content, id, score, timestamp, tags |
| `context` | `context(query: str, limit: int = 5)` | Ready-to-inject markdown block (same format as the agent adapters' `inject_context`) |
| `profile` | `profile()` | User profile summary + important facts |
| `stats` | `stats()` | JSON store statistics (`db_path`, row counts, tables) |
| `forget` | `forget(memory_id: str)` | `true`/`false` (false when the id does not exist) |

## Configuration

The server resolves its LanceDB store directory in this order:

1. **`OPENMEM_DB_PATH`** environment variable (hard override — use this to
   run against an isolated/test store),
2. `config.json` → `"memory" → "db_path"`,
3. `<repo>/data/lancedb`.

LLM reflection is heuristic-only unless API keys are configured (see
README.md, "Enabling AI Features"); the MCP tools themselves never need keys.

## Claude Code

One-liner:

```bash
claude mcp add openmem -- <path-to-python> -m mcp_server
```

Equivalent JSON (`.mcp.json` in the project root, or
`~/.claude.json` under `mcpServers`):

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

## Cursor

`~/.cursor/mcp.json` (Windows: `%USERPROFILE%\.cursor\mcp.json`):

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

Restart Cursor after saving; the OpenMem tools appear under MCP tool lists
as `openmem.remember`, `openmem.recall`, etc.

## Generic MCP client

Any client that speaks MCP over stdio:

```json
{
  "command": "<path-to-python>",
  "args": ["-m", "mcp_server"],
  "cwd": "<openmem-repo>",
  "env": {
    "OPENMEM_DB_PATH": "D:\\my\\openmem\\store"
  }
}
```

Or programmatically with the same SDK the tests use:

```python
import asyncio, os, sys
from mcp import ClientSession, StdioServerParameters
from mcp.client.stdio import stdio_client

async def main():
    params = StdioServerParameters(
        command=sys.executable,
        args=["-m", "mcp_server"],
        cwd=r"<openmem-repo>",
        env={**os.environ, "OPENMEM_DB_PATH": r"D:\my\openmem\store"},
    )
    async with stdio_client(params) as (read, write):
        async with ClientSession(read, write) as session:
            await session.initialize()
            result = await session.call_tool(
                "remember", {"content": "Prefers concise answers", "importance": 0.8}
            )
            print(result.content[0].text)

asyncio.run(main())
```

## Isolation & hygiene

- The integration suite (`tests/test_mcp_server.py`) spawns the real server
  as a subprocess per test with `OPENMEM_DB_PATH` pointed at a temp dir; the
  live `data/lancedb` store is never touched.
- While serving, all Python-level stdout writes are diverted to stderr, so
  library banner prints cannot corrupt the JSON-RPC stream.
- `stats().db_path` always reports which store the server is using — if you
  ever see the live path while you expected isolation, check that
  `OPENMEM_DB_PATH` actually reached the subprocess (`env` blocks REPLACE
  rather than merge in some clients).

## Troubleshooting

| Symptom | Fix |
|---|---|
| Client shows no tools | Run `python -m mcp_server` manually; it should block silently (logs on stderr). Check `mcp` is installed in THAT interpreter. |
| `ModuleNotFoundError: mcp` | `pip install -e ".[mcp]"` (or `uv pip install mcp` into your env). |
| Memories missing on another machine | The store is local filesystem state; point both clients at the same `OPENMEM_DB_PATH`/config. |
