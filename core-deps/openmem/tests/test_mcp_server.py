"""
Gate M1 — OpenMem MCP server end-to-end integration tests.

Spawns the real server (`python -m mcp_server`) as an IN-PROCESS-SUBPROCESS
via the installed `mcp` SDK's stdio client, pointed at a temporary
OPENMEM_DB_PATH store, and exercises EVERY tool through the actual protocol:

    remember -> recall returns it
             -> context contains it (adapter inject_context format)
             -> stats/profile well-formed
             -> forget removes it

SDK notes (verified empirically against mcp 2.0.0):
- The high-level server API is `mcp.server.MCPServer` (FastMCP's successor);
  there is no `mcp.server.fastmcp` module in 2.x.
- CallToolResult exposes `is_error` / snake_case fields.
"""

import unittest
import os
import sys
import json
import shutil
import tempfile
import asyncio
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent))

REPO_ROOT = Path(__file__).parent.parent


def call_text(result) -> str:
    """Extract the first text block from a CallToolResult."""
    for block in result.content:
        if getattr(block, "text", None) is not None:
            return block.text
    return ""


class TestDefaultDbPathResolution(unittest.TestCase):
    """Unit tests for the OPENMEM_DB_PATH / config.json resolution chain."""

    def test_env_var_overrides_everything(self):
        from memory_store.vector_db import _resolve_default_db_path
        old = os.environ.get("OPENMEM_DB_PATH")
        try:
            os.environ["OPENMEM_DB_PATH"] = os.path.join("some", "where")
            resolved = _resolve_default_db_path()
        finally:
            if old is None:
                os.environ.pop("OPENMEM_DB_PATH", None)
            else:
                os.environ["OPENMEM_DB_PATH"] = old
        self.assertEqual(resolved, os.path.abspath(os.path.join("some", "where")))

    def test_config_json_fallback_used_when_env_unset(self):
        """Without the env var, config.json memory.db_path (or the default)
        is returned — never raises even if config.json were malformed."""
        from memory_store.vector_db import _resolve_default_db_path
        old = os.environ.pop("OPENMEM_DB_PATH", None)
        try:
            resolved = _resolve_default_db_path()
        finally:
            if old is not None:
                os.environ["OPENMEM_DB_PATH"] = old
        self.assertIsInstance(resolved, str)
        self.assertTrue(len(resolved) > 0)


class TestMCPServerEndToEnd(unittest.TestCase):
    """
    Every test drives the real server over stdio in a fresh subprocess,
    against a fresh temporary LanceDB store (OPENMEM_DB_PATH).
    """

    def setUp(self):
        self.test_dir = tempfile.mkdtemp(prefix="openmem_mcp_test_")

    def tearDown(self):
        shutil.rmtree(self.test_dir, ignore_errors=True)

    def _server_params(self):
        from mcp import StdioServerParameters
        env = dict(os.environ)
        env["OPENMEM_DB_PATH"] = os.path.join(self.test_dir, "lancedb")
        return StdioServerParameters(
            command=sys.executable,
            args=["-m", "mcp_server"],
            cwd=str(REPO_ROOT),
            env=env,
        )

    def with_session(self, coro_fn):
        """Run coro_fn(session) against a freshly spawned server."""
        from mcp import ClientSession
        from mcp.client.stdio import stdio_client

        async def _runner():
            async with stdio_client(self._server_params()) as (read, write):
                async with ClientSession(read, write) as session:
                    init = await session.initialize()
                    return init, await coro_fn(session)

        return asyncio.run(_runner())[1]

    # ------------------------------------------------------------------
    # Protocol surface
    # ------------------------------------------------------------------

    def test_initialize_lists_all_six_tools(self):
        def body(session):
            return session.list_tools()

        result = self.with_session(body)
        names = sorted(t.name for t in result.tools)
        self.assertEqual(
            names, ["context", "forget", "profile", "recall", "remember", "stats"]
        )
        for tool in result.tools:
            self.assertTrue(tool.description, f"{tool.name} missing description")

    # ------------------------------------------------------------------
    # remember -> recall
    # ------------------------------------------------------------------

    def test_remember_then_recall_roundtrip(self):
        marker = "quantum flux capacitor calibration constant is 4242"

        def body(session):
            async def run():
                remembered = await session.call_tool(
                    "remember",
                    {"content": marker, "importance": 0.9, "tags": ["physics"]},
                )
                self.assertFalse(remembered.is_error, call_text(remembered))
                memory_id = call_text(remembered)
                self.assertTrue(len(memory_id) >= 8, memory_id)

                recalled = await session.call_tool(
                    "recall", {"query": "flux capacitor constant"}
                )
                return memory_id, call_text(recalled)

            return run()

        memory_id, recall_text = self.with_session(body)
        self.assertIn(marker, recall_text)
        self.assertIn(memory_id, recall_text)

    def test_remember_rejects_empty_content(self):
        def body(session):
            async def run():
                res = await session.call_tool("remember", {"content": "   "})
                text = call_text(res)
                return res.is_error or text.startswith("Error"), text

            return run()

        failed, text = self.with_session(body)
        self.assertTrue(failed, f"empty content should fail, got: {text}")

    def test_recall_hit_includes_score_timestamp_and_tags(self):
        marker = "zephyr engine torque specification table revision 7"

        def body(session):
            async def run():
                await session.call_tool(
                    "remember",
                    {"content": marker, "importance": 0.7,
                     "tags": ["engines", "specs"]},
                )
                recalled = await session.call_tool(
                    "recall", {"query": "zephyr torque specification"}
                )
                return call_text(recalled)

            return run()

        text = self.with_session(body)
        self.assertIn(marker, text)
        self.assertIn("score:", text)
        self.assertIn("timestamp:", text)
        self.assertIn("engines, specs", text)
        self.assertRegex(text, r"score: \d+\.\d{4}")

    # ------------------------------------------------------------------
    # context (adapter inject_context format)
    # ------------------------------------------------------------------

    def test_context_block_mirrors_adapter_format(self):
        marker = "helios launch codes are kept beside the coffee machine"

        def body(session):
            async def run():
                await session.call_tool(
                    "remember",
                    {"content": marker, "importance": 0.95},
                )
                ctx = await session.call_tool(
                    "context", {"query": "launch codes coffee"}
                )
                return call_text(ctx)

            return run()

        text = self.with_session(body)
        # Exact shape produced by AgentAdapter.format_memory_context.
        self.assertIn("## Relevant Memory Context", text)
        self.assertRegex(text, r"### 1\. \[memory\] \(importance: 0\.95\)")
        self.assertIn(marker[:300], text)

    def test_context_empty_query_returns_empty_string(self):
        def body(session):
            async def run():
                ctx = await session.call_tool("context", {"query": ""})
                return call_text(ctx)

            return run()

        self.assertEqual(self.with_session(body).strip(), "")

    # ------------------------------------------------------------------
    # profile / stats
    # ------------------------------------------------------------------

    def test_profile_is_well_formed_markdown(self):
        def body(session):
            async def run():
                prof = await session.call_tool("profile", {})
                return call_text(prof)

            return run()

        text = self.with_session(body)
        self.assertIn("## User Profile", text)
        self.assertTrue(len(text.strip()) > 0)

    def test_stats_reflects_isolated_store_and_rows(self):
        marker = "solstice archive retention policy document alpha"

        def body(session):
            async def run():
                await session.call_tool("remember", {"content": marker})
                raw = await session.call_tool("stats", {})
                return call_text(raw)

            return run()

        raw = self.with_session(body)
        payload = json.loads(raw)  # stats returns valid JSON
        self.assertEqual(
            payload["db_path"], os.path.join(self.test_dir, "lancedb"),
            "stats must report the OPENMEM_DB_PATH-isolated store"
        )
        self.assertGreaterEqual(payload["total_memories"], 1)
        self.assertIsInstance(payload["tables"], list)

    # ------------------------------------------------------------------
    # forget
    # ------------------------------------------------------------------

    def test_forget_removes_memory_end_to_end(self):
        marker = "obsidian mirror alignment rune sequence nine"

        def body(session):
            async def run():
                mid = call_text(await session.call_tool(
                    "remember",
                    {"content": marker, "tags": ["runes"]},
                ))

                forgotten = await session.call_tool(
                    "forget", {"memory_id": mid}
                )
                self.assertEqual(call_text(forgotten), "true")

                gone = await session.call_tool(
                    "recall", {"query": "obsidian mirror rune"}
                )
                return call_text(gone)

            return run()

        self.assertEqual(self.with_session(body), "No results.")

    def test_forget_nonexistent_id_reports_false(self):
        def body(session):
            async def run():
                res = await session.call_tool(
                    "forget", {"memory_id": "deadbeefdeadbeef"}
                )
                return call_text(res)

            return run()

        self.assertEqual(self.with_session(body), "false")

    # ------------------------------------------------------------------
    # Importance clamping
    # ------------------------------------------------------------------

    def test_remember_clamps_out_of_range_importance(self):
        marker = "aurora borealis ticket inventory ledger entry zero"

        def body(session):
            async def run():
                await session.call_tool(
                    "remember",
                    {"content": marker, "importance": 42.0},
                )
                ctx = await session.call_tool(
                    "context", {"query": "aurora borealis ledger"}
                )
                return call_text(ctx)

            return run()

        text = self.with_session(body)
        self.assertRegex(text, r"\(importance: 1\.00\)")

    # ------------------------------------------------------------------
    # Store isolation proof
    # ------------------------------------------------------------------

    def test_server_writes_only_to_isolated_store_directory(self):
        def body(session):
            async def run():
                await session.call_tool(
                    "remember", {"content": "isolation probe epsilon"}
                )
                return call_text(await session.call_tool("stats", {}))

            return run()

        payload = json.loads(self.with_session(body))
        self.assertNotIn(
            "data\\lancedb", payload["db_path"].lower().replace("/", "\\"),
            "server must not touch the live store when OPENMEM_DB_PATH is set"
        )


if __name__ == "__main__":
    unittest.main()
