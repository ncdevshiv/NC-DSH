"""
Unit tests for real agent session parsers (Phase 1).

Fixtures replicate the EXACT record layouts inventoried on this machine
(see doc/session_formats.md). No copies of real conversations are committed —
all fixtures are synthetic strings that mirror field-for-field structure:

- Claude Code: ~/.claude/projects/<munged-cwd>/<uuid>.jsonl typed records
- Codex CLI:   ~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl payload records
- Cursor:      ~/.cursor/sessions/*.json (tolerant; no live data observed)
"""

import io
import json
import os
import sys
import time
import shutil
import tempfile
import unittest
import contextlib
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent))

from agents.base import AgentAdapter
from agents.claude_code.adapter import ClaudeCodeAdapter
from agents.codex_cli.adapter import CodexCliAdapter
from agents.cursor.adapter import CursorAdapter


def _write_jsonl(path: Path, records) -> str:
    """Write records (dicts or raw strings for malformed lines) as JSONL."""
    path.parent.mkdir(parents=True, exist_ok=True)
    lines = []
    for rec in records:
        if isinstance(rec, str):
            lines.append(rec)
        else:
            lines.append(json.dumps(rec))
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")
    return str(path)


class TestJsonlHelpers(unittest.TestCase):
    """Shared base-class helpers used by every parser."""

    def setUp(self):
        self.test_dir = tempfile.mkdtemp()

    def tearDown(self):
        shutil.rmtree(self.test_dir, ignore_errors=True)

    def test_read_jsonl_records_counts_malformed(self):
        fpath = os.path.join(self.test_dir, "mixed.jsonl")
        with open(fpath, "w", encoding="utf-8") as f:
            f.write(json.dumps({"type": "user"}) + "\n")
            f.write("{broken json\n")
            f.write("\n")  # blank lines are ignored silently
            f.write(json.dumps({"type": "assistant"}) + "\n")

        records, malformed = AgentAdapter.read_jsonl_records(fpath)
        self.assertEqual(len(records), 2)
        self.assertEqual(malformed, 1)

    def test_find_session_files_recursive_and_window(self):
        root = os.path.join(self.test_dir, "2026", "08", "21")
        os.makedirs(root, exist_ok=True)
        new_file = os.path.join(root, "rollout-new.jsonl")
        old_file = os.path.join(root, "rollout-old.jsonl")
        for fp in (new_file, old_file):
            Path(fp).write_text("{}\n", encoding="utf-8")
        # Push the old file far outside any hours_back window
        old_ts = time.time() - (400 * 3600)
        os.utime(old_file, (old_ts, old_ts))

        found = AgentAdapter.find_session_files(
            self.test_dir, "**/rollout-*.jsonl", hours_back=168
        )
        self.assertEqual([os.path.basename(f) for f in found], ["rollout-new.jsonl"])


class TestClaudeCodeParser(unittest.TestCase):
    """Parsers for ~/.claude/projects/<dir>/<uuid>.jsonl (exact layout)."""

    SID_A = "11111111-2222-3333-4444-555555555555"
    SID_B = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee"

    def setUp(self):
        self.test_dir = tempfile.mkdtemp()
        self.claude_dir = os.path.join(self.test_dir, ".claude")
        proj_dir = Path(self.claude_dir) / "projects" / "F--test-proj"

        # Session A: newest file, full noise mix mirroring a real transcript
        self.session_a = _write_jsonl(proj_dir / f"{self.SID_A}.jsonl", [
            # queue-operation duplicates the user prompt -> must NOT index
            {"type": "queue-operation", "operation": "enqueue",
             "timestamp": "2026-08-21T15:02:24.460Z", "sessionId": self.SID_A,
             "content": "what are pros and cons of the harness approach"},
            # genuine user turn (string content)
            {"parentUuid": None, "isSidechain": False,
             "promptId": "b4f5a88d-9766-4704-9e35-8df62400dcd9",
             "type": "user",
             "message": {"role": "user",
                         "content": "what are pros and cons of the harness approach"},
             "uuid": "958dbf94-4525-403b-962d-75241bc47899",
             "timestamp": "2026-08-21T15:02:24.681Z",
             "cwd": "F:\\test-proj", "sessionId": self.SID_A,
             "version": "2.1.237", "gitBranch": "main"},
            # CLI command echo -> filtered
            {"type": "user",
             "message": {"role": "user",
                         "content": "<command-name>/model</command-name>\n<command-message>model</command-message>"},
             "timestamp": "2026-08-21T15:02:43.190Z", "sessionId": self.SID_A},
            # local-command stdout echo -> filtered
            {"type": "user",
             "message": {"role": "user",
                         "content": "<local-command-stdout>Set model to opus</local-command-stdout>"},
             "timestamp": "2026-08-21T15:02:43.190Z", "sessionId": self.SID_A},
            # synthetic assistant API-error record -> filtered
            {"parentUuid": "x", "isSidechain": False, "type": "assistant",
             "uuid": "ad95b1e8", "timestamp": "2026-08-21T15:02:28.170Z",
             "message": {"id": "0352043e", "model": "<synthetic>",
                         "role": "assistant", "stop_reason": "stop_sequence",
                         "type": "message",
                         "content": [{"type": "text",
                                      "text": "Failed to authenticate. API Error: 401"}]},
             "sessionId": self.SID_A},
            # real assistant reply (block-array content)
            {"parentUuid": "y", "isSidechain": False, "type": "assistant",
             "uuid": "ad95b1e9", "timestamp": "2026-08-21T15:03:00.000Z",
             "message": {"id": "msg_1", "model": "claude-opus-4-8-20261014",
                         "role": "assistant", "stop_reason": "stop_sequence",
                         "type": "message",
                         "content": [{"type": "text",
                                      "text": "The harness model centralizes control flow."},
                                     {"type": "text",
                                      "text": "Main con: migration cost."}]},
             "sessionId": self.SID_A},
            # sidechain subagent record -> excluded by default
            {"parentUuid": "z", "isSidechain": True, "type": "user",
             "message": {"role": "user", "content": "sidechain-only task"},
             "timestamp": "2026-08-21T15:04:00.000Z", "sessionId": self.SID_A},
            # user array content: text block + tool_result block
            {"type": "user",
             "message": {"role": "user",
                         "content": [{"tool_use_id": "t1", "type": "tool_result",
                                      "content": [{"type": "text", "text": "tool spam"}]},
                                     {"type": "text",
                                      "text": "thanks, the porting plan works"}]},
             "timestamp": "2026-08-21T15:05:00.000Z", "sessionId": self.SID_A},
            # assistant tool_use-only array (no text blocks) -> dropped
            {"type": "assistant",
             "message": {"id": "msg_2", "model": "claude-opus-4-8-20261014",
                         "role": "assistant",
                         "content": [{"type": "tool_use", "id": "tu1",
                                      "name": "Bash", "input": {}}]},
             "timestamp": "2026-08-21T15:06:00.000Z", "sessionId": self.SID_A},
            # metadata/noise types -> skipped
            {"type": "attachment", "attachment": {"type": "agent_listing_delta"}},
            {"type": "last-prompt", "lastPrompt": "dup prompt", "sessionId": self.SID_A},
            {"type": "atis-latch", "atis": "", "sessionId": self.SID_A},
            {"type": "custom-title", "customTitle": "title", "sessionId": self.SID_A},
            {"type": "mode", "mode": "normal", "sessionId": self.SID_A},
            # malformed line -> skipped + counted
            '{"type":"user","message":{"role":"user","con',
        ])

        # Session B: older file with one plain exchange
        self.session_b = _write_jsonl(proj_dir / f"{self.SID_B}.jsonl", [
            {"type": "user",
             "message": {"role": "user", "content": "hello there adapter test"},
             "timestamp": "2026-08-20T10:00:00.000Z", "sessionId": self.SID_B},
            {"type": "assistant",
             "message": {"id": "msg_9", "model": "claude-opus-4-8-20261014",
                         "role": "assistant",
                         "content": [{"type": "text", "text": "greeting acknowledged"}]},
             "timestamp": "2026-08-20T10:00:05.000Z", "sessionId": self.SID_B},
        ])
        # Make session B clearly older than session A
        old_ts = time.time() - (2 * 3600)
        os.utime(self.session_b, (old_ts, old_ts))

        self.adapter = ClaudeCodeAdapter(
            workspace=self.test_dir, claude_dir=self.claude_dir
        )

    def tearDown(self):
        shutil.rmtree(self.test_dir, ignore_errors=True)

    def test_parses_user_and_assistant_messages(self):
        messages = self.adapter.get_session_messages(limit=100)
        roles = [m["role"] for m in messages]
        self.assertIn("user", roles)
        self.assertIn("assistant", roles)
        contents = " ".join(m["content"] for m in messages)
        self.assertIn("pros and cons of the harness approach", contents)
        self.assertIn("harness model centralizes control flow", contents)

    def test_filters_noise_records(self):
        messages = self.adapter.get_session_messages(limit=100)
        contents = " ".join(m["content"] for m in messages)
        # synthetic auth error
        self.assertNotIn("Failed to authenticate", contents)
        # command echoes
        self.assertNotIn("/model", contents)
        self.assertNotIn("Set model to opus", contents)
        # queue-operation duplicate must not double the prompt
        prompt_count = sum(
            1 for m in messages
            if m["role"] == "user"
            and "pros and cons of the harness approach" in m["content"]
        )
        self.assertEqual(prompt_count, 1)
        # sidechain excluded
        self.assertNotIn("sidechain-only task", contents)

    def test_block_array_content_extraction(self):
        messages = self.adapter.get_session_messages(limit=100)
        joined = "\n".join(m["content"] for m in messages)
        # text blocks concatenated...
        self.assertIn("centralizes control flow.\nMain con: migration cost.", joined)
        # ...while tool_result-only noise is not treated as conversation
        self.assertNotIn("tool spam", joined)
        # user array message keeps its text block only
        self.assertIn("the porting plan works", joined)

    def test_session_id_and_timestamp_carry_through(self):
        messages = self.adapter.get_session_messages(limit=100)
        first_user = next(m for m in messages if m["role"] == "user")
        self.assertEqual(first_user["session_id"], self.SID_A)
        self.assertEqual(first_user["timestamp"], "2026-08-21T15:02:24.681Z")

    def test_tolerates_malformed_lines_with_debug_count(self):
        buf = io.StringIO()
        with contextlib.redirect_stdout(buf):
            messages = self.adapter.get_session_messages(limit=100)
        output = buf.getvalue()
        self.assertEqual(len(messages), 5)  # 4 from A + 2 from B - nothing lost? checked below
        self.assertIn("skipped 1 malformed line(s)", output)

    def test_hours_back_window_excludes_old_files(self):
        # Session B was utime'd 2h back; push it beyond the 1h window
        old_ts = time.time() - (2 * 3600)
        os.utime(self.session_b, (old_ts, old_ts))
        buf = io.StringIO()
        with contextlib.redirect_stdout(buf):
            messages = self.adapter.get_session_messages(limit=100, hours_back=1)
        sids = {m["session_id"] for m in messages}
        self.assertEqual(sids, {self.SID_A})

    def test_newest_first_ordering(self):
        messages = self.adapter.get_session_messages(limit=100)
        self.assertEqual(messages[0]["session_id"], self.SID_A)

    def test_get_recent_sessions_groups_by_session(self):
        sessions = self.adapter.get_recent_sessions(hours_back=168, limit=100)
        ids = {s["id"] for s in sessions}
        self.assertEqual(ids, {self.SID_A, self.SID_B})
        counts = {s["id"]: len(s["messages"]) for s in sessions}
        self.assertEqual(counts[self.SID_B], 2)


class TestCodexCliParser(unittest.TestCase):
    """Parsers for ~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl."""

    ROLLOUT_SID = "01a024d1-cc8e-7fa2-8d77-5283e1ae5189"

    def setUp(self):
        self.test_dir = tempfile.mkdtemp()
        self.codex_dir = os.path.join(self.test_dir, ".codex")
        rollout = (Path(self.codex_dir) / "sessions" / "2026" / "08" / "21" /
                   f"rollout-2026-08-21T20-25-24-{self.ROLLOUT_SID}.jsonl")
        _write_jsonl(rollout, [
            {"timestamp": "2026-08-21T14:55:24.746Z", "ordinal": 0,
             "type": "session_meta",
             "payload": {"session_id": self.ROLLOUT_SID,
                         "id": self.ROLLOUT_SID,
                         "timestamp": "2026-08-21T14:55:24.304Z",
                         "cwd": "C:\\Temp", "originator": "codex_exec",
                         "cli_version": "0.149.0", "source": "exec"}},
            {"timestamp": "2026-08-21T14:55:24.746Z", "ordinal": 1,
             "type": "event_msg",
             "payload": {"type": "task_started", "turn_id": "t1"}},
            # developer injection -> skipped
            {"timestamp": "2026-08-21T14:55:26.196Z", "ordinal": 2,
             "type": "response_item",
             "payload": {"type": "message", "id": "msg_1", "role": "developer",
                         "content": [{"type": "input_text",
                                      "text": "<skills_instructions> be helpful"}]}},
            # machine-injected user context -> filtered
            {"timestamp": "2026-08-21T14:55:26.196Z", "ordinal": 3,
             "type": "response_item",
             "payload": {"type": "message", "id": "msg_2", "role": "user",
                         "content": [{"type": "input_text",
                                      "text": "<recommended_plugins> Airtable etc"}]}},
            # genuine user question (multi-block)
            {"timestamp": "2026-08-21T14:55:27.851Z", "ordinal": 4,
             "type": "response_item",
             "payload": {"type": "message", "id": "msg_3", "role": "user",
                         "content": [{"type": "input_text",
                                      "text": "how do I port my codebase"},
                                     {"type": "input_text",
                                      "text": "to the harness principle?"}]}},
            # assistant answer
            {"timestamp": "2026-08-21T14:55:30.375Z", "ordinal": 5,
             "type": "response_item",
             "payload": {"type": "message", "id": "msg_4", "role": "assistant",
                         "content": [{"type": "output_text",
                                      "text": "Start with an orchestrator kernel."}]}},
            {"timestamp": "2026-08-21T14:55:30.400Z", "ordinal": 6,
             "type": "turn_context",
             "payload": {"turn_id": "t1", "cwd": "C:\\Temp"}},
            {"timestamp": "2026-08-21T14:55:31.000Z", "ordinal": 7,
             "type": "world_state", "payload": {"full": True, "state": {}}},
            '{"timestamp":"2026-08-21T14:55:32.000Z","ordinal":8,"type":"respo',
        ])
        self.adapter = CodexCliAdapter(
            workspace=self.test_dir, codex_dir=self.codex_dir
        )

    def tearDown(self):
        shutil.rmtree(self.test_dir, ignore_errors=True)

    def test_parses_rollout_messages(self):
        messages = self.adapter.get_session_messages(limit=100)
        self.assertEqual(len(messages), 2)
        self.assertEqual(messages[0]["role"], "user")
        self.assertIn("how do I port my codebase\nto the harness principle?",
                      messages[0]["content"])
        self.assertEqual(messages[1]["role"], "assistant")
        self.assertIn("orchestrator kernel", messages[1]["content"])

    def test_filters_injected_context_and_developer_roles(self):
        messages = self.adapter.get_session_messages(limit=100)
        joined = " ".join(m["content"] for m in messages)
        self.assertNotIn("skills_instructions", joined)
        self.assertNotIn("recommended_plugins", joined)

    def test_session_id_from_session_meta(self):
        messages = self.adapter.get_session_messages(limit=100)
        for m in messages:
            self.assertEqual(m["session_id"], self.ROLLOUT_SID)
            self.assertTrue(m["timestamp"].startswith("2026-08-21T14:55"))

    def test_tolerates_malformed_lines_with_debug_count(self):
        buf = io.StringIO()
        with contextlib.redirect_stdout(buf):
            self.adapter.get_session_messages(limit=100)
        self.assertIn("skipped 1 malformed line(s)", buf.getvalue())


class TestCursorParser(unittest.TestCase):
    """Cursor discovery is tolerant; no live format observed on this machine."""

    def setUp(self):
        self.test_dir = tempfile.mkdtemp()
        self.cursor_dir = os.path.join(self.test_dir, ".cursor")

    def tearDown(self):
        shutil.rmtree(self.test_dir, ignore_errors=True)

    def test_empty_sessions_returns_no_messages(self):
        os.makedirs(os.path.join(self.cursor_dir, "sessions"), exist_ok=True)
        adapter = CursorAdapter(workspace=self.test_dir, cursor_dir=self.cursor_dir)
        self.assertEqual(adapter.get_session_messages(limit=10), [])

    def test_single_doc_json_messages(self):
        sess = os.path.join(self.cursor_dir, "sessions", "chat-1.json")
        Path(sess).parent.mkdir(parents=True, exist_ok=True)
        Path(sess).write_text(json.dumps({
            "messages": [
                {"role": "user", "content": "explain the build failure please",
                 "timestamp": "2026-08-21T12:00:00Z"},
                {"role": "assistant", "content": "The missing import caused it.",
                 "timestamp": "2026-08-21T12:00:05Z"},
            ]
        }), encoding="utf-8")
        adapter = CursorAdapter(workspace=self.test_dir, cursor_dir=self.cursor_dir)
        messages = adapter.get_session_messages(limit=10)
        self.assertEqual(len(messages), 2)
        self.assertEqual(messages[0]["session_id"], "chat-1")
        self.assertIn("build failure", messages[0]["content"])

    def test_missing_home_dir_returns_empty(self):
        adapter = CursorAdapter(
            workspace=self.test_dir,
            cursor_dir=os.path.join(self.test_dir, "does-not-exist"),
        )
        self.assertEqual(adapter.get_session_messages(limit=10), [])


class _FakeAdapter(AgentAdapter):
    """Minimal adapter stub feeding pre-parsed sessions to the indexer."""

    AGENT_NAME = "FakeAgent"

    def __init__(self, sessions):
        self._sessions = sessions

    def get_session_messages(self, limit: int = 100):
        msgs = []
        for s in self._sessions:
            msgs.extend(s["messages"])
        return msgs[:limit]

    def get_recent_sessions(self, hours_back: int = 168, limit: int = 100):
        return self._sessions

    def inject_context(self, context: str) -> bool:
        return True

    def get_workspace_path(self) -> str:
        return "/tmp"

    def get_session_id(self) -> str:
        return "fake-session"

    def get_agent_name(self) -> str:
        return self.AGENT_NAME

    def get_skill_install_path(self):
        return None


class TestAdapterDrivenIndexer(unittest.TestCase):
    """ConversationIndexer consumes adapter sessions and dedups by state."""

    def setUp(self):
        self.test_dir = tempfile.mkdtemp()
        from memory_store.vector_db import VectorDB
        self.vdb = VectorDB(db_path=os.path.join(self.test_dir, "vectordb"))
        self.state_file = os.path.join(self.test_dir, "state", "index_state.json")
        self.sessions = [
            {"id": "sess-one", "path": "/tmp/one.jsonl", "messages": [
                {"role": "user", "content": "please refactor the ingest pipeline",
                 "timestamp": "2026-08-21T10:00:00Z", "session_id": "sess-one"},
                {"role": "assistant", "content": "Refactored the ingest pipeline.",
                 "timestamp": "2026-08-21T10:00:10Z", "session_id": "sess-one"},
            ]},
            {"id": "sess-two", "path": "/tmp/two.jsonl", "messages": [
                {"role": "user", "content": "now add regression tests for it",
                 "timestamp": "2026-08-21T11:00:00Z", "session_id": "sess-two"},
            ]},
        ]

    def tearDown(self):
        self.vdb.close()
        shutil.rmtree(self.test_dir, ignore_errors=True)

    def _make_indexer(self):
        from learning_loop.conversation_indexer import ConversationIndexer
        return ConversationIndexer(
            adapter=_FakeAdapter(self.sessions),
            vector_db=self.vdb,
            index_state_file=self.state_file,
        )

    @staticmethod
    def _memory_rows(vdb) -> int:
        stats = vdb.get_stats()
        return stats.get("table_memories_rows", 0)

    def test_indexes_adapter_sessions(self):
        indexer = self._make_indexer()
        report = indexer.run_indexing(hours_back=24)
        self.assertEqual(report["sessions_found"], 2)
        self.assertEqual(report["sessions_indexed"], 2)
        self.assertEqual(report["messages_indexed"], 3)
        self.assertEqual(report["adapter"], "FakeAgent")
        self.assertEqual(sorted(report["newly_indexed_sessions"]),
                         ["sess-one", "sess-two"])
        # Newly indexed messages are exposed for the reflection phase
        self.assertEqual(set(indexer.last_new_session_messages.keys()),
                         {"sess-one", "sess-two"})
        self.assertGreaterEqual(self._memory_rows(self.vdb), 3)

    def test_second_run_does_not_double_count(self):
        indexer = self._make_indexer()
        first = indexer.run_indexing(hours_back=24)
        second = indexer.run_indexing(hours_back=24)
        self.assertEqual(first["messages_indexed"], 3)
        self.assertEqual(second["messages_indexed"], 0)
        self.assertEqual(second["newly_indexed_sessions"], [])
        self.assertEqual(self._memory_rows(self.vdb), 3)
        # Composite keys recorded in persistent state
        state = json.loads(Path(self.state_file).read_text(encoding="utf-8"))
        self.assertIn("fakeagent:sess-one", state["indexed_sessions"])
        self.assertIn("fakeagent:sess-two", state["indexed_sessions"])

    def test_state_survives_reindexer_restart(self):
        indexer = self._make_indexer()
        indexer.run_indexing(hours_back=24)
        # Fresh instance simulates a new process reading persisted state
        indexer2 = self._make_indexer()
        report = indexer2.run_indexing(hours_back=24)
        self.assertEqual(report["messages_indexed"], 0)


if __name__ == "__main__":
    unittest.main()
