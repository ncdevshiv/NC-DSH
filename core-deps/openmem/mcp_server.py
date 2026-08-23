"""
OpenMem MCP Server — native Model Context Protocol integration.

Exposes the OpenMem memory layer as MCP tools so any MCP-capable client
(Claude Code, Cursor, and generic MCP hosts) can remember, recall, and
forget memories directly.

Built on the installed `mcp` SDK's high-level server API. The task brief
names FastMCP; in the installed SDK generation (mcp 2.x) that API lives as
`mcp.server.MCPServer` with the same decorator-based tool registration
(verified empirically against mcp 2.0.0 — there is no
`mcp.server.fastmcp` module anymore).

Run:
    python -m mcp_server                 # stdio transport (default)
    openmem-mcp                          # console script entry point

Configuration:
    OPENMEM_DB_PATH    Hard override for the LanceDB store directory.
                       Falls back to config.json "memory.db_path", then to
                       <repo>/data/lancedb. Required by the integration
                       test suite for store isolation.

Tools:
    remember(content, importance=0.5, tags=[]) -> memory id
    recall(query, limit=5)                     -> formatted hits
    context(query, limit=5)                    -> ready-to-inject markdown
    profile()                                  -> user profile summary
    stats()                                    -> store statistics
    forget(memory_id)                          -> True/False
"""

import os
import sys
import json
import logging
from contextlib import asynccontextmanager, redirect_stdout
from pathlib import Path
from typing import List, Optional

BASE_DIR = Path(__file__).parent
sys.path.insert(0, str(BASE_DIR))

logger = logging.getLogger("openmem.mcp")

SERVER_NAME = "openmem"
SERVER_VERSION = "2.0.0"

# Module-level singletons, initialized lazily so importing this module has
# no side effects (safe for tests and for tooling that only wants main).
_store = None
_user_model = None


def _shielded(fn, *args, **kwargs):
    """
    Run fn() with stdout diverted to stderr.

    LanceDBVectorStore construction prints connection banners to stdout;
    before the stdio transport claims the stream those prints would land on
    the wire and corrupt the MCP handshake (verified empirically on mcp
    2.0.0 / Windows).
    """
    with redirect_stdout(sys.stderr):
        return fn(*args, **kwargs)


def get_store():
    """Get or create the shared vector store singleton."""
    global _store
    if _store is None:
        from memory_store.vector_db import get_vector_db
        _store = _shielded(get_vector_db)
    return _store


def get_user_model():
    """Get or create the user model singleton."""
    global _user_model
    if _user_model is None:
        from memory_store.user_model import UserModel
        _user_model = UserModel()
    return _user_model


@asynccontextmanager
async def _stdout_guard(server):
    """
    Lifespan hook: while serving, route Python-level stdout writes to stderr.

    The transport captures its own handle to the real stdout at startup, so
    swapping sys.stdout here cannot affect protocol framing — but it does
    catch stray print() calls from library code inside tool handlers, which
    on Windows are NOT caught by the SDK's fd-level diversion.
    """
    real_stdout = sys.stdout
    sys.stdout = sys.stderr
    try:
        yield {}
    finally:
        sys.stdout = real_stdout


def build_server():
    """Construct the MCPServer instance with all tools registered."""
    from mcp.server import MCPServer

    server = MCPServer(
        name=SERVER_NAME,
        version=SERVER_VERSION,
        instructions=(
            "OpenMem persistent memory for AI agents. Use remember() to "
            "store durable facts or preferences, recall()/context() to "
            "retrieve relevant history before answering, and forget() to "
            "remove a memory by id."
        ),
        lifespan=_stdout_guard,
    )

    @server.tool(description=(
        "Store a memory (fact, preference, decision) for later recall. "
        "Returns the new memory id."
    ))
    def remember(content: str, importance: float = 0.5,
                 tags: Optional[List[str]] = None) -> str:
        content = (content or "").strip()
        if not content:
            return "Error: content must be a non-empty string"
        importance = min(1.0, max(0.0, float(importance)))
        memory_id = get_store().add_memory(
            content=content,
            importance=importance,
            tags=list(tags or []),
            metadata={"source": "mcp"},
        )
        if not memory_id:
            return "Error: failed to store memory"
        return memory_id

    @server.tool(description=(
        "Search stored memories semantically. Returns formatted hits with "
        "score, timestamp, and tags."
    ))
    def recall(query: str, limit: int = 5) -> str:
        query = (query or "").strip()
        if not query:
            return "No results."
        results = get_store().search(query, n_results=max(1, int(limit)))
        if not results:
            return "No results."
        lines = []
        for i, r in enumerate(results, 1):
            score = r.get("score")
            score_text = f"{float(score):.4f}" if score is not None else "n/a"
            meta_ts = (r.get("metadata") or {}).get("timestamp", "")
            timestamp = r.get("timestamp") or meta_ts or ""
            lines.append(f"{i}. {r.get('content', '')}")
            lines.append(f"   id: {r.get('id')} | score: {score_text} | "
                         f"timestamp: {timestamp}")
            tags = r.get("tags") or []
            if tags:
                lines.append(f"   tags: {', '.join(str(t) for t in tags)}")
        return "\n".join(lines)

    @server.tool(description=(
        "Return a ready-to-inject markdown context block of memories "
        "relevant to the query (mirrors the agent adapters' inject_context "
        "format)."
    ))
    def context(query: str, limit: int = 5) -> str:
        query = (query or "").strip()
        if not query:
            return ""
        results = get_store().search(query, n_results=max(1, int(limit)))
        # Same shape as AgentAdapter.format_memory_context (agents/base.py)
        lines = ["## Relevant Memory Context\n"]
        for i, mem in enumerate(results, 1):
            content = mem.get("content", "")[:300]
            tier = mem.get("metadata", {}).get("tier", "memory")
            importance = mem.get("importance", 0)
            lines.append(f"### {i}. [{tier}] (importance: {importance:.2f})")
            lines.append(content)
            lines.append("")
        profile_block = _format_profile_context()
        if profile_block:
            lines.append(profile_block)
        return "\n".join(lines).strip() + "\n"

    @server.tool(description="Summarize what OpenMem knows about the user.")
    def profile() -> str:
        summary = get_user_model().get_profile_summary()
        facts = get_user_model().profile.get("important_facts", {})
        lines = ["## User Profile\n"]
        lines.append(summary or "No profile data yet.")
        if facts:
            lines.append("\n### Important Facts")
            for key, data in list(facts.items())[:20]:
                confidence = data.get("confidence", 0)
                lines.append(f"- **{key}**: {data.get('value')} ({confidence:.0%})")
        return "\n".join(lines)

    @server.tool(description="OpenMem store statistics (tables, row counts).")
    def stats() -> str:
        store_stats = get_store().get_stats()
        payload = {
            "db_path": store_stats.get("db_path"),
            "total_memories": store_stats.get("total_memories"),
            "total_user_profiles": store_stats.get("total_user_profiles"),
            "tables": store_stats.get("tables"),
            "embedder_available": store_stats.get("embedder_available"),
        }
        return json.dumps(payload, indent=2)

    @server.tool(description="Delete a memory by id. Returns True on success.")
    def forget(memory_id: str) -> bool:
        memory_id = (memory_id or "").strip()
        if not memory_id:
            return False
        return bool(get_store().delete_memory(memory_id))

    return server


def _format_profile_context() -> str:
    """
    Format the user-profile section with the adapters' exact line shape
    (AgentAdapter.format_user_profile_context in agents/base.py).
    """
    try:
        facts = get_user_model().profile.get("important_facts", {})
        if not facts:
            return ""
        lines = ["## User Profile\n"]
        for key, data in facts.items():
            value = data.get("value", str(data))
            confidence = float(data.get("confidence", 0))
            lines.append(f"- **{key}**: {value} ({confidence:.0%})")
        return "\n".join(lines)
    except Exception as e:  # Profile decoration must never break context()
        logger.debug(f"[MCP] Profile context skipped: {e}")
        return ""


def main() -> int:
    """Console-script / python -m entry point (stdio transport)."""
    server = build_server()
    # Pre-construct the store under a shield so LanceDB banner prints can
    # never race the transport for fd 1.
    get_store()
    print(f"[MCP] Starting OpenMem MCP server '{SERVER_NAME}' v{SERVER_VERSION}",
          file=sys.stderr)
    server.run(transport="stdio")
    return 0


if __name__ == "__main__":
    sys.exit(main())
