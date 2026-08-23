"""
Outcome-grounded improvement queue enforcement tests.

Phase-3 contract under test:

1. Items identified by reflection enter "pending" and NEVER carry
   completion evidence at identification time.
2. pending -> completed REQUIRES evidence (at least one of
   evidence_memory_id / evidence_session_id / confirmed_by="user");
   completing without evidence raises ValueError and the item stays
   pending. This kills the original millisecond-self-complete bug where
   run_self_check identified, applied, and completed an item in one call
   with zero evidence (visible in data/improvements.json history as
   identical identified_at/completed_at timestamps).
3. Evidence survives the JSON round trip through improvements.json.
"""

import json
import os
import shutil
import sys
import tempfile
import unittest
from datetime import datetime
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent))

from learning_loop.reflection_engine import ReflectionEngine
from memory_store.memory_manager import MemoryManager
from memory_store.user_model import UserModel
from memory_store.vector_db import VectorDB


def _make_pending_improvement():
    return {
        "type": "knowledge_gap",
        "description": "User asked about unknown topic",
        "priority": "medium",
        "identified_at": datetime.now().isoformat(),
    }


class TestOutcomeEvidence(unittest.TestCase):
    """Evidence-gated pending -> completed transitions."""

    def setUp(self):
        """Isolated engine: all state stores rebound into a temp dir."""
        self.test_dir = tempfile.mkdtemp()
        self.engine = ReflectionEngine()
        tmp = os.path.join(self.test_dir, "engine_data")
        os.makedirs(tmp, exist_ok=True)
        self.engine.reflection_log = os.path.join(tmp, "reflections.json")
        self.improvements_file = os.path.join(tmp, "improvements.json")
        self.engine.improvements_file = self.improvements_file
        self.engine.reflections = {
            "session_reflections": [],
            "cross_session_reflections": [],
            "corrections_made": [],
            "last_reflection": None,
        }
        self.engine.improvements = {"pending": [], "completed": [], "rejected": []}
        self.engine.vector_db = VectorDB(db_path=os.path.join(tmp, "vectordb"))
        self.engine.memory_manager = MemoryManager(base_path=os.path.join(tmp, "memory"))
        self.engine.user_model = UserModel(base_path=os.path.join(tmp, "usermodel"))
        self.engine.user_model.vector_db = self.engine.vector_db

    def tearDown(self):
        try:
            self.engine.vector_db.close()
            self.engine.memory_manager.close()
        except Exception:
            pass
        shutil.rmtree(self.test_dir, ignore_errors=True)

    def _queue_one(self):
        imp = _make_pending_improvement()
        self.engine.improvements["pending"].append(dict(imp))
        self.engine._save_improvements()
        return imp

    # ----- refusal cases: item must STAY PENDING -----

    def test_completion_without_evidence_raises_and_stays_pending(self):
        imp = self._queue_one()
        with self.assertRaises(ValueError):
            self.engine.complete_improvement(imp)
        self.assertIn(imp, self.engine.improvements["pending"])
        self.assertEqual(self.engine.improvements["completed"], [])

    def test_whitespace_only_evidence_is_rejected(self):
        imp = self._queue_one()
        with self.assertRaises(ValueError):
            self.engine.complete_improvement(
                imp, evidence_memory_id="   ", evidence_session_id=""
            )
        self.assertIn(imp, self.engine.improvements["pending"])

    def test_non_user_confirmed_by_is_rejected(self):
        """Only confirmed_by='user' counts; the engine cannot confirm itself."""
        imp = self._queue_one()
        for bogus in ("engine", "self", "assistant", "user ", "USER"):
            with self.assertRaises(ValueError):
                self.engine.complete_improvement(imp, confirmed_by=bogus)
        self.assertIn(imp, self.engine.improvements["pending"])
        self.assertEqual(len(self.engine.improvements["pending"]), 1,
                         "rejected attempts must not duplicate or drop the item")

    def test_unknown_improvement_with_valid_evidence_returns_false(self):
        ghost = _make_pending_improvement()  # never queued
        result = self.engine.complete_improvement(
            ghost, evidence_memory_id="mem_x"
        )
        self.assertFalse(result)
        self.assertEqual(self.engine.improvements["completed"], [])

    # ----- acceptance cases: completes WITH evidence -----

    def test_completes_with_evidence_memory_id(self):
        imp = self._queue_one()
        ok = self.engine.complete_improvement(imp, evidence_memory_id="mem_abc123")
        self.assertTrue(ok)
        self.assertNotIn(imp, self.engine.improvements["pending"])
        record = self.engine.improvements["completed"][0]
        self.assertEqual(record["evidence_memory_id"], "mem_abc123")
        self.assertNotIn("confirmed_by", record)
        self.assertIn("completed_at", record)

    def test_completes_with_evidence_session_id(self):
        imp = self._queue_one()
        ok = self.engine.complete_improvement(
            imp, evidence_session_id="sess-2026-08-22-001"
        )
        self.assertTrue(ok)
        self.assertEqual(
            self.engine.improvements["completed"][0]["evidence_session_id"],
            "sess-2026-08-22-001",
        )

    def test_completes_with_explicit_user_confirmation(self):
        imp = self._queue_one()
        ok = self.engine.complete_improvement(imp, confirmed_by="user")
        self.assertTrue(ok)
        record = self.engine.improvements["completed"][0]
        self.assertEqual(record["confirmed_by"], "user")

    def test_multiple_evidence_kinds_all_recorded(self):
        imp = self._queue_one()
        ok = self.engine.complete_improvement(
            imp,
            evidence_memory_id="mem_def456",
            evidence_session_id="sess-777",
            confirmed_by="user",
        )
        self.assertTrue(ok)
        record = self.engine.improvements["completed"][0]
        self.assertEqual(record["evidence_memory_id"], "mem_def456")
        self.assertEqual(record["evidence_session_id"], "sess-777")
        self.assertEqual(record["confirmed_by"], "user")

    # ----- persistence -----

    def test_evidence_survives_json_round_trip(self):
        imp = self._queue_one()
        self.engine.complete_improvement(
            imp,
            evidence_memory_id="mem_rt_001",
            evidence_session_id="sess_rt_001",
        )
        with open(self.improvements_file, "r", encoding="utf-8") as f:
            persisted = json.load(f)
        self.assertEqual(persisted["pending"], [])
        record = persisted["completed"][0]
        self.assertEqual(record["evidence_memory_id"], "mem_rt_001")
        self.assertEqual(record["evidence_session_id"], "sess_rt_001")
        self.assertIn("completed_at", record)


class TestNoAutoComplete(unittest.TestCase):
    """run_self_check must never complete items by itself anymore."""

    def setUp(self):
        self.test_dir = tempfile.mkdtemp()
        self.engine = ReflectionEngine()
        tmp = os.path.join(self.test_dir, "engine_data")
        os.makedirs(tmp, exist_ok=True)
        self.engine.reflection_log = os.path.join(tmp, "reflections.json")
        self.engine.improvements_file = os.path.join(tmp, "improvements.json")
        self.engine.reflections = {
            "session_reflections": [],
            "cross_session_reflections": [],
            "corrections_made": [],
            "last_reflection": None,
        }
        self.engine.improvements = {"pending": [], "completed": [], "rejected": []}
        self.engine.vector_db = VectorDB(db_path=os.path.join(tmp, "vectordb"))
        self.engine.memory_manager = MemoryManager(base_path=os.path.join(tmp, "memory"))
        self.engine.user_model = UserModel(base_path=os.path.join(tmp, "usermodel"))
        self.engine.user_model.vector_db = self.engine.vector_db

    def tearDown(self):
        try:
            self.engine.vector_db.close()
            self.engine.memory_manager.close()
        except Exception:
            pass
        shutil.rmtree(self.test_dir, ignore_errors=True)

    def test_run_self_check_applies_but_never_completes(self):
        self.engine.improvements["pending"].append(_make_pending_improvement())
        report = self.engine.run_self_check()

        self.assertEqual(report["improvements_completed"], 0,
                         "auto-completion must stay at zero without evidence")
        self.assertEqual(report.get("improvements_applied"), 1)
        self.assertIn("improvements_note", report)
        self.assertEqual(len(self.engine.improvements["pending"]), 1,
                         "applied item must remain pending for evidenced sign-off")
        self.assertEqual(self.engine.improvements["completed"], [])

    def test_identification_paths_never_attach_evidence(self):
        """Reflection identifies gaps; identification itself fabricates nothing."""
        messages = [
            {"role": "user", "content": "What is quantum entanglement?",
             "session_id": "ident_sess"},
        ]
        reflection = self.engine.reflect_on_session(messages)

        # The heuristic path must have produced at least one queueable gap.
        gap_items = [
            i for i in reflection.get("improvements_identified", [])
            if i.get("type") == "knowledge_gap"
        ]
        self.assertGreater(len(gap_items), 0)

        # Whatever reached the queue: no evidence fields at identification.
        self.assertGreater(len(self.engine.improvements["pending"]), 0)
        for pending in self.engine.improvements["pending"]:
            self.assertNotIn("evidence_memory_id", pending)
            self.assertNotIn("evidence_session_id", pending)
            self.assertNotIn("confirmed_by", pending)
            self.assertNotIn("completed_at", pending)


if __name__ == "__main__":
    unittest.main()
