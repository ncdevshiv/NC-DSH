"""
Tests for OpenMem memory_store module.
"""

import unittest
import os
import sys
import tempfile
import shutil
import json
from pathlib import Path
from datetime import datetime

# Add parent to path
sys.path.insert(0, str(Path(__file__).parent.parent))

from memory_store.vector_db import VectorDB


class TestVectorDB(unittest.TestCase):
    """Tests for VectorDB class."""
    
    def setUp(self):
        """Set up test fixtures."""
        self.test_dir = tempfile.mkdtemp()
        self.db = VectorDB(db_path=os.path.join(self.test_dir, "test_vectordb"))
    
    def tearDown(self):
        """Clean up test fixtures."""
        self.db.close()
        shutil.rmtree(self.test_dir, ignore_errors=True)
    
    def test_add_memory(self):
        """Test adding a memory."""
        memory_id = self.db.add_memory(
            content="This is a test memory",
            session_id="test_session_1",
            importance=0.7,
            tags=["test", "example"]
        )
        
        self.assertIsNotNone(memory_id)
        self.assertIsInstance(memory_id, str)
    
    def test_search_text(self):
        """Test text-based search."""
        self.db.add_memory(content="Python is a programming language", session_id="s1")
        self.db.add_memory(content="JavaScript is for web development", session_id="s2")
        self.db.add_memory(content="I love coding in Python", session_id="s3")
        
        results = self.db.search("Python", n_results=5)
        
        self.assertIsInstance(results, list)
        # Should find memories containing "Python"
        python_contents = [r['content'] for r in results if 'python' in r['content'].lower()]
        self.assertGreater(len(python_contents), 0)
    
    def test_search_session_filter(self):
        """Test search with session filter."""
        self.db.add_memory(content="Memory one", session_id="session_a")
        self.db.add_memory(content="Memory two", session_id="session_b")
        self.db.add_memory(content="Memory three", session_id="session_a")
        
        results = self.db.search("Memory", session_id="session_a", n_results=10)
        
        self.assertIsInstance(results, list)
        # All results should be from session_a
        for r in results:
            if r.get('session_id'):
                self.assertEqual(r['session_id'], "session_a")
    
    def test_get_recent_memories(self):
        """Test getting recent memories."""
        self.db.add_memory(content="Recent memory", importance=0.8)
        self.db.add_memory(content="Older memory", importance=0.5)
        
        recent = self.db.get_recent_memories(hours=24, limit=10)
        
        self.assertIsInstance(recent, list)
        self.assertGreaterEqual(len(recent), 2)
    
    def test_user_profile(self):
        """Test user profile operations."""
        self.db.set_user_profile("name", "TestUser", confidence=0.9)
        
        profile = self.db.get_user_profile("name")
        
        self.assertIsNotNone(profile)
        self.assertEqual(profile['value'], "TestUser")
        self.assertEqual(profile['confidence'], 0.9)
    
    def test_get_all_user_profiles(self):
        """Test getting all user profiles."""
        self.db.set_user_profile("name", "Alice", confidence=0.8)
        self.db.set_user_profile("color", "blue", confidence=0.7)
        
        profiles = self.db.get_all_user_profiles()
        
        self.assertIsInstance(profiles, dict)
        self.assertIn("name", profiles)
        self.assertIn("color", profiles)
    
    def test_update_importance(self):
        """Test updating memory importance."""
        memory_id = self.db.add_memory(content="Test memory", importance=0.5)
        
        self.db.update_importance(memory_id, 0.9)
        
        recent = self.db.get_recent_memories(hours=24, limit=10)
        updated = [r for r in recent if r['id'] == memory_id]
        
        if updated:
            self.assertEqual(updated[0]['importance'], 0.9)
    
    def test_get_stats(self):
        """Test getting database stats."""
        self.db.add_memory(content="Memory 1", importance=0.6)
        self.db.add_memory(content="Memory 2", importance=0.7)
        self.db.set_user_profile("name", "Test")
        
        stats = self.db.get_stats()
        
        self.assertIn("total_memories", stats)
        self.assertIn("total_user_profiles", stats)
        self.assertGreaterEqual(stats["total_memories"], 2)
    
    def test_add_memory_explicit_id(self):
        """Explicit memory_id wins; generated ids stay the default."""
        explicit = self.db.add_memory(content="Fixture A", memory_id="golden-x-01")
        auto = self.db.add_memory(content="Fixture B")
        self.assertEqual(explicit, "golden-x-01")
        self.assertIsInstance(auto, str)
        self.assertNotEqual(auto, "golden-x-01")

        row = self.db.get_memory("golden-x-01")
        self.assertIsNotNone(row)
        self.assertEqual(row["content"], "Fixture A")

    def test_add_memories_batch(self):
        """Test batch memory addition."""
        memories = [
            {"content": "Batch memory 1", "importance": 0.6},
            {"content": "Batch memory 2", "importance": 0.7},
            {"content": "Batch memory 3", "importance": 0.8, "session_id": "batch_session"}
        ]
        
        ids = self.db.add_memories_batch(memories)
        
        self.assertEqual(len(ids), 3)
        self.assertTrue(all(isinstance(i, str) for i in ids))


class TestMemoryManager(unittest.TestCase):
    """Tests for MemoryManager class."""
    
    def setUp(self):
        """Set up test fixtures."""
        self.test_dir = tempfile.mkdtemp()
        from memory_store.memory_manager import MemoryManager
        self.manager = MemoryManager(base_path=os.path.join(self.test_dir, "memory"))
        # Isolation: store_*() mirrors every tier entry into vector_db, which
        # defaults to the shared live singleton. Redirect it to a temp-backed
        # store so tier tests never add rows to data/lancedb.
        self.manager.vector_db = VectorDB(db_path=os.path.join(self.test_dir, "vectordb"))
    
    def tearDown(self):
        """Clean up test fixtures."""
        self.manager.close()
        shutil.rmtree(self.test_dir, ignore_errors=True)
    
    def test_store_daily_memory(self):
        """Test storing daily memory."""
        today = datetime.now().strftime("%Y-%m-%d")
        memory_id = self.manager.store_daily_memory(today, "Test daily memory content")
        
        self.assertIsNotNone(memory_id)
        self.assertTrue(memory_id.startswith("daily_"))
    
    def test_store_weekly_summary(self):
        """Test storing weekly summary."""
        week_start = "2026-04-01"
        memory_id = self.manager.store_weekly_summary(week_start, "This week's summary content")
        
        self.assertIsNotNone(memory_id)
        self.assertTrue(memory_id.startswith("weekly_"))
    
    def test_store_longterm_memory(self):
        """Test storing long-term memory."""
        memory_id = self.manager.store_longterm_memory("important_fact", "The sky is blue", confidence=0.95)
        
        self.assertIsNotNone(memory_id)
        self.assertTrue(memory_id.startswith("longterm_"))
    
    def test_get_daily_memories(self):
        """Test retrieving daily memories."""
        today = datetime.now().strftime("%Y-%m-%d")
        self.manager.store_daily_memory(today, "Daily memory 1")
        self.manager.store_daily_memory(today, "Daily memory 2")
        
        memories = self.manager.get_daily_memories(today)
        
        self.assertIsInstance(memories, list)
        self.assertEqual(len(memories), 2)
    
    def test_get_longterm_memories(self):
        """Test retrieving long-term memories."""
        self.manager.store_longterm_memory("fact1", "Content 1")
        self.manager.store_longterm_memory("fact2", "Content 2")
        
        memories = self.manager.get_longterm_memories()
        
        self.assertIsInstance(memories, list)
        self.assertEqual(len(memories), 2)
    
    def test_search_memory(self):
        """Test memory search."""
        self.manager.store_daily_memory("2026-04-01", "Python programming is fun")
        
        results = self.manager.search_memory("Python")
        
        self.assertIsInstance(results, list)
    
    def test_run_consolidation(self):
        """Test memory consolidation."""
        # Create some daily memories
        # (store_daily_memory has no importance kwarg; importance is fixed at 0.6)
        self.manager.store_daily_memory("2026-03-25", "Memory from last week")
        
        report = self.manager.run_consolidation(dry_run=False)
        
        self.assertIn("weekly_created", report)
        self.assertIn("longterm_created", report)


if __name__ == "__main__":
    unittest.main()
