"""
Cross-session content dedup tests (Phase 2 hygiene, deliverable H-b).

The indexer computes normalized sha256(content.strip().lower()) per
candidate message and skips any message whose hash was already indexed in
ANY prior session (hash set persisted in index_state.json). Per-session
re-index idempotency (agent:id state keys) must remain untouched.
"""

import unittest
import os
import sys
import json
import shutil
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent))

from learning_loop.conversation_indexer import ConversationIndexer
from memory_store.vector_db import VectorDB


class _FakeAdapter:
    """Minimal adapter stand-in returning canned sessions."""

    AGENT_NAME = "Test Agent"

    def __init__(self, sessions=None):
        self.sessions = sessions or []

    def get_agent_name(self):
        return self.AGENT_NAME

    def get_recent_sessions(self, hours_back=24, limit=100):
        return self.sessions


def _session(sid, texts):
    return {
        "id": sid,
        "path": "",
        "messages": [
            {"role": "user", "content": t, "timestamp": "", "session_id": sid}
            for t in texts
        ],
    }


class TestCrossSessionContentDedup(unittest.TestCase):

    def setUp(self):
        self.test_dir = tempfile.mkdtemp()
        self.state_file = os.path.join(self.test_dir, "index_state.json")
        self.store = VectorDB(db_path=os.path.join(self.test_dir, "vectordb"))

    def tearDown(self):
        self.store.close()
        shutil.rmtree(self.test_dir, ignore_errors=True)

    def _indexer(self, sessions):
        return ConversationIndexer(
            vector_db=self.store,
            index_state_file=self.state_file,
            adapter=_FakeAdapter(sessions),
        )

    def test_same_content_in_second_session_is_skipped(self):
        idx = self._indexer([
            _session("s1", ["Unique alpha message about lances"]),
            _session("s2", ["Unique alpha message about lances"]),
        ])
        # Both sessions arrive in one scan; s2's copy collapses onto s1's.
        r1 = idx.run_indexing(hours_back=24)
        self.assertEqual(r1["messages_indexed"], 1)
        self.assertEqual(r1["duplicates_skipped"], 1)
        self.assertEqual(len(self.store), 1)

        # A later rescan rediscovers both sessions but the state keys
        # short-circuit: no work, no duplicate churn.
        r2 = idx.run_indexing(hours_back=24)
        self.assertEqual(r2["messages_indexed"], 0)
        self.assertEqual(r2["duplicates_skipped"], 0)

    def test_normalization_ignores_case_and_surrounding_whitespace(self):
        idx = self._indexer([
            _session("s1", ["Remember The Deployment Window"]),
            _session("s2", ["  remember the deployment window  "]),
        ])
        report = idx.run_indexing(hours_back=24)
        self.assertEqual(report["messages_indexed"], 1)
        self.assertEqual(report["duplicates_skipped"], 1,
                         "case/whitespace variants must collapse to one hash")

    def test_distinct_content_still_indexed(self):
        idx = self._indexer([
            _session("s1", ["First distinct memory body"]),
            _session("s2", ["Second totally different body"]),
        ])
        report = idx.run_indexing(hours_back=24)
        self.assertEqual(report["messages_indexed"], 2)
        self.assertEqual(report["duplicates_skipped"], 0)
        self.assertEqual(len(self.store), 2)

    def test_session_marked_indexed_even_when_all_content_seen(self):
        """A fully-duplicated session must be marked so future scans do no
        repeated work, without adding reflection input."""
        idx = self._indexer([
            _session("s1", ["Shared sentence repeated later"]),
            _session("s2", ["Shared sentence repeated later"]),
        ])
        idx.run_indexing(hours_back=24)
        state_keys = idx.index_state["indexed_sessions"]
        agent_key = idx._agent_key()
        self.assertIn(f"{agent_key}:s1", state_keys)
        self.assertIn(f"{agent_key}:s2", state_keys)
        # Nothing new from s2 reached the reflection feed
        self.assertNotIn("s2", idx.last_new_session_messages)

    def test_persisted_hash_set_survives_restarts(self):
        first = self._indexer([_session("s1", ["Persistent dedup probe"])])
        first.run_indexing(hours_back=24)

        # Fresh indexer instance (simulated process restart) sees s2 with
        # the same content; the persisted hash set must skip it.
        second = self._indexer([_session("s2", ["Persistent dedup probe"])])
        report = second.run_indexing(hours_back=24)
        self.assertEqual(report["messages_indexed"], 0)
        self.assertEqual(report["duplicates_skipped"], 1)

        with open(self.state_file, "r", encoding="utf-8") as f:
            raw_state = json.load(f)
        self.assertIn("content_hashes", raw_state)
        self.assertTrue(raw_state["content_hashes"])

    def test_per_session_reindex_idempotency_untouched(self):
        idx = self._indexer([_session("s1", ["Idempotency guard content"])])
        r1 = idx.run_indexing(hours_back=24)
        self.assertEqual(r1["sessions_indexed"], 1)
        # Second pass: the agent:id state key short-circuits before any
        # hashing happens.
        r2 = idx.run_indexing(hours_back=24)
        self.assertEqual(r2["messages_indexed"], 0)
        self.assertEqual(r2["sessions_indexed"], 0)
        self.assertEqual(r2["duplicates_skipped"], 0)
        self.assertEqual(len(self.store), 1)

    def test_reindex_all_resets_hashes_for_full_rebuild(self):
        idx = self._indexer([_session("s1", ["Rebuild target content"])])
        idx.run_indexing(hours_back=24)

        idx.index_state["indexed_sessions"] = []
        idx.index_state["content_hashes"] = []
        report = idx.run_indexing(hours_back=24)
        self.assertEqual(report["messages_indexed"], 1)

    def test_backfill_seeds_only_going_forward(self):
        """First run after upgrade: hash set starts EMPTY (no retroactive
        scan of history) but a migration marker records the cutover."""
        idx = self._indexer([])
        self.assertEqual(idx.index_state.get("content_hashes"), [])
        self.assertIn("content_hash_dedup_since",
                      idx.index_state,
                      "upgrade marker missing; forward-only backfill not recorded")


if __name__ == "__main__":
    unittest.main()
