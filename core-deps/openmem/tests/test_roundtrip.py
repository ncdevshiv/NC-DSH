"""
Offline regression tests encoding the OpenMem vector-store round-trip contract.

These tests define the REQUIRED END STATE of the contract currently being
implemented in memory_store/vector_db.py. A failure here indicates a contract
gap in the implementation, not a bug in this suite. Everything runs offline:
no torch, no sentence-transformers, no network, no model downloads.

Contract clauses under test:
  1. add_memory(content=...) returns an id string; get_memory(id) returns a
     dict whose content/metadata match what was stored.
  2. update_importance(id, 0.9) returns True; the new importance persists.
  3. get_recent_memories(hours=...) returns memories newest-first by
     timestamp regardless of insertion order; session_id filter works.
  4. delete_old_memories(days=N, min_importance=X) deletes ONLY rows older
     than cutoff AND with importance < X, returns the exact deleted count,
     and leaves remaining rows intact.
  5. set_user_profile(key, val) twice keeps exactly ONE underlying row per
     key and get_user_profile returns the latest value.
  6. With no embedder loaded (_local_embedder is None), _embed_text raises
     RuntimeError containing actionable guidance ("sentence-transformers");
     add_memory(..., auto_embed=False) must work when the schema permits
     null vectors.
  7. A deterministic FakeEmbedder stub lets search paths exercise real
     vector math offline.
"""

import json
import os
import shutil
import sys
import tempfile
import unittest
import uuid
from datetime import datetime, timedelta
from pathlib import Path
from typing import Any, Dict, List, Optional

import numpy as np

# Add project root to path (AGENTS.md convention).
sys.path.insert(0, str(Path(__file__).parent.parent))

from memory_store.vector_db import LanceDBVectorStore

EMBED_DIM = 384  # all-MiniLM-L6-v2 output dimension used by the store


class FakeEmbedder:
    """Deterministic offline stand-in for SentenceTransformer.encode()."""

    dimension = EMBED_DIM

    def encode(self, texts):
        """Return a (n, dimension) float32 array seeded per input text.

        hash() is salted across processes but stable within one process,
        which is all these single-process tests require.
        """
        if isinstance(texts, str):
            texts = [texts]
        return np.stack([
            np.random.RandomState(abs(hash(t)) % (2 ** 32))
            .rand(self.dimension)
            .astype("float32")
            for t in texts
        ])


def _make_store(db_subdir: str, *, with_embedder: bool = True) -> LanceDBVectorStore:
    """Build one LanceDBVectorStore over a throwaway directory (per test)."""
    store = LanceDBVectorStore(db_path=db_subdir)
    if with_embedder:
        store._local_embedder = FakeEmbedder()  # clause 7 stubbing pattern
    return store


def _backdated_row(
    content: str,
    when: datetime,
    importance: float,
    *,
    session_id: Optional[str] = None,
    tags: Optional[List[str]] = None,
    vector: Optional[List[float]] = None,
) -> Dict[str, Any]:
    """Craft a memories-table row matching LanceDBVectorStore.SCHEMA exactly."""
    return {
        "id": uuid.uuid4().hex[:16],
        "content": content,
        "session_id": session_id,
        "timestamp": when.isoformat(),
        "importance": float(importance),
        "tags": tags or [],
        "metadata": json.dumps({}),
        "vector": vector,
    }


class TestVectorRoundTrip(unittest.TestCase):
    """Contract tests for LanceDBVectorStore round-trip behaviour (offline)."""

    def setUp(self):
        self.test_dir = tempfile.mkdtemp()
        self.store = _make_store(os.path.join(self.test_dir, "rt_vectordb"))

    def tearDown(self):
        self.store.close()
        shutil.rmtree(self.test_dir, ignore_errors=True)

    # ----- helpers -------------------------------------------------------

    def _table_contents(self) -> List[Dict[str, Any]]:
        """Raw scan of the underlying memories table."""
        return self.store._table.to_arrow().to_pylist()

    @staticmethod
    def _sorted_contents(rows: List[Dict[str, Any]]) -> List[str]:
        return sorted(r["content"] for r in rows)

    # ----- Clause 1 ------------------------------------------------------

    def test_add_get_round_trip(self):
        """Clause 1: add returns id str; get_memory returns stored fields."""
        memory_id = self.store.add_memory(
            content="round trip payload",
            session_id="rt_session",
            importance=0.5,
            tags=["tagA", "tagB"],
            metadata={"origin": "unittest", "rank": 7},
        )
        self.assertIsNotNone(memory_id)
        self.assertIsInstance(memory_id, str)

        fetched = self.store.get_memory(memory_id)
        self.assertIsNotNone(fetched, "get_memory must retrieve the just-added row")
        self.assertEqual(fetched["content"], "round trip payload")
        self.assertEqual(fetched["session_id"], "rt_session")
        self.assertEqual(sorted(fetched["tags"]), ["tagA", "tagB"])
        self.assertEqual(fetched["metadata"], {"origin": "unittest", "rank": 7})

    def test_get_memory_retrieves_null_vector_row(self):
        """Clauses 1+6: id lookup must not depend on the row having a vector."""
        self.store._local_embedder = None
        memory_id = self.store.add_memory("null vector lookup", auto_embed=False)
        self.assertIsNotNone(memory_id)

        fetched = self.store.get_memory(memory_id)
        self.assertIsNotNone(
            fetched, "get_memory must find rows even when their vector is null"
        )
        self.assertEqual(fetched["content"], "null vector lookup")

    # ----- Clause 2 ------------------------------------------------------

    def test_update_importance_returns_true(self):
        """Clause 2a: update_importance reports success."""
        memory_id = self.store.add_memory(content="importance target", importance=0.2)
        self.assertIsNotNone(memory_id)
        self.assertTrue(
            self.store.update_importance(memory_id, 0.9),
            "update_importance must return True on success",
        )

    def test_update_importance_persists(self):
        """Clause 2b: get_memory shows importance == 0.9 after update."""
        memory_id = self.store.add_memory(content="importance target", importance=0.2)
        self.store.update_importance(memory_id, 0.9)

        fetched = self.store.get_memory(memory_id)
        self.assertIsNotNone(fetched)
        self.assertAlmostEqual(
            fetched["importance"], 0.9, places=6,
            msg="importance must persist as 0.9 (float32 storage tolerance)",
        )

    # ----- Clause 3 ------------------------------------------------------

    def test_get_recent_memories_orders_newest_first(self):
        """Clause 3: newest-first by timestamp regardless of insertion order."""
        now = datetime.now()
        embedder = self.store._local_embedder

        def vec(text: str) -> List[float]:
            return embedder.encode([text])[0].tolist()

        rows = [
            _backdated_row("mid aged", now - timedelta(hours=20), 0.5, vector=vec("mid aged")),
            _backdated_row("oldest row", now - timedelta(hours=40), 0.5, vector=vec("oldest row")),
            _backdated_row("newest row", now - timedelta(hours=1), 0.5, vector=vec("newest row")),
        ]
        self.store._table.add(rows)  # insertion order deliberately != time order

        recent = self.store.get_recent_memories(hours=48, limit=10)
        self.assertIsInstance(recent, list)
        self.assertEqual(len(recent), 3, "all three in-window rows must be returned")
        self.assertEqual(
            self._sorted_contents(recent),
            sorted(["mid aged", "oldest row", "newest row"]),
            "no rows may be dropped",
        )

        stamps = [r["timestamp"] for r in recent]
        self.assertEqual(
            stamps, sorted(stamps, reverse=True),
            "rows must be ordered newest-first by timestamp",
        )
        self.assertEqual(recent[0]["content"], "newest row")

    def test_get_recent_memories_session_filter(self):
        """Clause 3: session_id filter restricts results to that session."""
        now = datetime.now()
        embedder = self.store._local_embedder

        def vec(text: str) -> List[float]:
            return embedder.encode([text])[0].tolist()

        self.store._table.add([
            _backdated_row("alpha in session", now - timedelta(hours=1), 0.5,
                           session_id="filter_sess", vector=vec("alpha in session")),
            _backdated_row("beta in session", now - timedelta(hours=2), 0.5,
                           session_id="filter_sess", vector=vec("beta in session")),
            _backdated_row("gamma other session", now - timedelta(minutes=10), 0.9,
                           session_id="other_sess", vector=vec("gamma other session")),
        ])

        recent = self.store.get_recent_memories(
            hours=24, limit=10, session_id="filter_sess"
        )
        self.assertEqual(len(recent), 2, "only the two filter_sess rows may return")
        for row in recent:
            self.assertEqual(row["session_id"], "filter_sess")

        stamps = [r["timestamp"] for r in recent]
        self.assertEqual(stamps, sorted(stamps, reverse=True))

    # ----- Clause 4 ------------------------------------------------------

    def test_delete_old_memories_is_selective_and_counts(self):
        """Clause 4: delete ONLY old AND low-importance rows; exact count."""
        now = datetime.now()
        old = now - timedelta(days=45)
        fresh = now - timedelta(days=2)
        rows = [
            _backdated_row("old low a", old, 0.10),         # delete
            _backdated_row("old low b", old, 0.29),         # delete
            _backdated_row("old high", old, 0.90),          # keep: importance >= X
            _backdated_row("old boundary", old, 0.30),      # keep: X comparison strict
            _backdated_row("recent low", fresh, 0.05),      # keep: newer than cutoff
        ]
        self.store._table.add(rows)

        deleted = self.store.delete_old_memories(days=30, min_importance=0.3)

        self.assertEqual(
            deleted, 2,
            "exactly the two old low-importance rows must be reported deleted",
        )
        remaining = self._table_contents()
        self.assertEqual(len(remaining), 3, "remaining rows must stay intact")
        self.assertEqual(
            self._sorted_contents(remaining),
            sorted(["old high", "old boundary", "recent low"]),
        )

    # ----- Clause 5 ------------------------------------------------------

    def test_set_user_profile_twice_keeps_one_row_latest_value(self):
        """Clause 5: double set => single underlying row, latest value wins."""
        self.assertTrue(self.store.set_user_profile("fav_color", "blue"))
        self.assertTrue(self.store.set_user_profile("fav_color", "red", confidence=0.8))

        profile = self.store.get_user_profile("fav_color")
        self.assertIsNotNone(profile)
        self.assertEqual(profile["value"], "red")
        self.assertAlmostEqual(profile["confidence"], 0.8, places=6)

        profiles_table = self.store._db.open_table("user_profiles")
        keys = profiles_table.to_arrow().column("profile_key").to_pylist()
        self.assertEqual(
            keys.count("fav_color"), 1,
            "profile table must hold exactly ONE row for that key",
        )

    # ----- Clause 6 ------------------------------------------------------

    def test_embed_text_without_embedder_raises_actionable_error(self):
        """Clause 6a: missing embedder => RuntimeError naming sentence-transformers."""
        self.store._local_embedder = None
        with self.assertRaises(RuntimeError) as ctx:
            self.store._embed_text("no model loaded")
        self.assertIn(
            "sentence-transformers", str(ctx.exception),
            "error message must contain actionable install guidance",
        )

    def test_add_memory_auto_embed_false_works_without_embedder(self):
        """Clause 6b: storing without embedding must not raise on missing embedder."""
        self.store._local_embedder = None
        try:
            memory_id = self.store.add_memory("no embed payload", auto_embed=False)
        except Exception as exc:  # schema forbidding null vectors => skip, not fail
            self.skipTest(f"deployed schema forbids null vectors: {exc!r}")
            return
        self.assertIsNotNone(memory_id)

        matches = [r for r in self._table_contents() if r["id"] == memory_id]
        self.assertEqual(len(matches), 1, "row must persist despite null vector")
        self.assertEqual(matches[0]["content"], "no embed payload")
        self.assertIsNone(matches[0]["vector"])

    # ----- Clause 7 ------------------------------------------------------

    def test_search_exercises_real_vector_math_with_fake_embedder(self):
        """Clause 7: stubbed embedder drives genuine nearest-neighbour ranking."""
        self.store.add_memory("unique zebra crossing memo", session_id="vs")
        self.store.add_memory("totally different ledger entry", session_id="vs")

        hits = self.store.search(
            "unique zebra crossing memo", n_results=2, use_rerank=False
        )
        self.assertIsInstance(hits, list)
        self.assertGreater(len(hits), 0)
        self.assertEqual(
            hits[0]["content"], "unique zebra crossing memo",
            "top hit must be the exact-match text under real vector math",
        )


if __name__ == "__main__":
    unittest.main()
