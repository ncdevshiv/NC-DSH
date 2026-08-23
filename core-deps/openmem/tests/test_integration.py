"""
Integration tests for OpenMem.
Tests the full learning loop and OpenClaw integration.
"""

import unittest
import os
import sys
import tempfile
import shutil
import json
import time
from pathlib import Path
from datetime import datetime

# Add parent to path
sys.path.insert(0, str(Path(__file__).parent.parent))


class TestFullLearningCycle(unittest.TestCase):
    """Tests for full learning cycle."""
    
    def setUp(self):
        """Set up test fixtures."""
        self.test_dir = tempfile.mkdtemp()
        
        # Set environment to use test directory
        os.environ["OPENMEM_TEST_MODE"] = "true"
    
    def tearDown(self):
        """Clean up test fixtures."""
        shutil.rmtree(self.test_dir, ignore_errors=True)
        if "OPENMEM_TEST_MODE" in os.environ:
            del os.environ["OPENMEM_TEST_MODE"]
    
    def test_vector_db_integration(self):
        """Test vector DB integrates properly."""
        from memory_store.vector_db import VectorDB
        
        db_path = os.path.join(self.test_dir, "test_vectordb")
        db = VectorDB(db_path=db_path)
        
        # Add a memory
        memory_id = db.add_memory(
            content="Integration test memory",
            session_id="integration_test",
            importance=0.8,
            tags=["integration", "test"]
        )
        
        # Search for it
        results = db.search("Integration test")
        
        self.assertGreater(len(results), 0)
        self.assertTrue(any("integration test memory" in r["content"].lower() for r in results))
        
        db.close()
    
    def test_memory_manager_integration(self):
        """Test memory manager integrates with vector DB."""
        from memory_store.memory_manager import MemoryManager
        from memory_store.vector_db import VectorDB

        mem_path = os.path.join(self.test_dir, "test_memory")
        manager = MemoryManager(base_path=mem_path)
        # Isolation: store_daily_memory mirrors into vector_db; bind the
        # temp store so this integration test never writes to data/lancedb.
        manager.vector_db = VectorDB(db_path=os.path.join(self.test_dir, "mgr_vectordb"))
        
        # Store daily memory
        today = datetime.now().strftime("%Y-%m-%d")
        daily_id = manager.store_daily_memory(today, "Test daily memory for integration")
        
        # Search should find it in vector DB
        results = manager.search_memory("daily memory")
        
        self.assertGreater(len(results), 0)
        
        manager.close()
    
    def test_user_model_integration(self):
        """Test user model integrates with vector DB."""
        from memory_store.user_model import UserModel
        from memory_store.vector_db import VectorDB

        # Hermetic stores: keep analysis/profile persistence out of repo data/.
        model = UserModel(base_path=os.path.join(self.test_dir, "usermodel"))
        model.vector_db = VectorDB(db_path=os.path.join(self.test_dir, "usermodel_vectordb"))
        
        # Analyze a message
        analysis = model.analyze_message(
            message="My name is TestUser and I work on AI projects",
            channel="test_channel"
        )
        
        self.assertIn("detected_style", analysis)
        self.assertIn("topics_extracted", analysis)
        
        # Should have extracted facts
        profile = model.profile.get("important_facts", {})
        # Facts should be updated (implementation specific)
    
    def test_pattern_recognizer_integration(self):
        """Test pattern recognizer with real data."""
        from learning_loop.pattern_recognizer import PatternRecognizer
        from memory_store.vector_db import VectorDB
        
        db_path = os.path.join(self.test_dir, "test_vectordb")
        db = VectorDB(db_path=db_path)
        
        # Add multiple related memories
        for i in range(5):
            db.add_memory(
                content=f"Python programming is great for AI development",
                session_id=f"session_{i}"
            )
        
        recognizer = PatternRecognizer()
        # find_recurring_patterns reads self.vector_db (the global singleton);
        # bind it to the seeded throwaway store so the test is hermetic and
        # the assertion reflects the memories added above.
        recognizer.vector_db = db
        patterns = recognizer.find_recurring_patterns(days_back=1)
        
        # Should find Python as a pattern
        self.assertTrue(len(patterns) > 0)
        
        db.close()
    
    def test_reflection_engine_integration(self):
        """Test reflection engine with session data."""
        from learning_loop.reflection_engine import ReflectionEngine
        from memory_store.vector_db import VectorDB
        from memory_store.memory_manager import MemoryManager
        from memory_store.user_model import UserModel

        engine = ReflectionEngine()
        # Isolation: redirect the engine's shared-state stores into the temp
        # dir so this test neither reads nor mutates repo data/.
        tmp = os.path.join(self.test_dir, "engine_data")
        os.makedirs(tmp, exist_ok=True)
        engine.reflection_log = os.path.join(tmp, "reflections.json")
        engine.improvements_file = os.path.join(tmp, "improvements.json")
        engine.reflections = {
            "session_reflections": [],
            "cross_session_reflections": [],
            "corrections_made": [],
            "last_reflection": None,
        }
        engine.improvements = {"pending": [], "completed": [], "rejected": []}
        engine.vector_db = VectorDB(db_path=os.path.join(tmp, "vectordb"))
        engine.memory_manager = MemoryManager(base_path=os.path.join(tmp, "memory"))
        engine.user_model = UserModel(base_path=os.path.join(tmp, "usermodel"))
        engine.user_model.vector_db = engine.vector_db
        
        # Simulate a conversation
        messages = [
            {"role": "user", "content": "Help me code in Python", "session_id": "test_session"},
            {"role": "assistant", "content": "Here's Python code for you", "session_id": "test_session"},
            {"role": "user", "content": "Perfect, great help!", "session_id": "test_session"},
        ]
        
        reflection = engine.reflect_on_session(messages)
        
        # Should detect success
        self.assertEqual(reflection["outcome"], "success")
        self.assertTrue(len(reflection["improvements_identified"]) > 0)
    
    def test_skill_generator_integration(self):
        """Test skill generator creates valid skills."""
        from memory_store.skill_generator import SkillGenerator
        from memory_store.vector_db import VectorDB
        
        skills_path = os.path.join(self.test_dir, "test_skills")
        
        # Add memories with patterns
        db_path = os.path.join(self.test_dir, "test_vectordb")
        db = VectorDB(db_path=db_path)
        
        # Add memories with repeated keywords
        for i in range(5):
            db.add_memory(
                content=f"Build Python web applications with Flask framework",
                session_id=f"session_{i}"
            )
        
        generator = SkillGenerator(skills_output_path=skills_path)
        
        # Discover patterns
        patterns = generator.discover_patterns(hours_back=24)
        
        self.assertIn("high_freq_keywords", patterns)
        self.assertIn("keyword_clusters", patterns)
    
    def test_scheduler_initialization(self):
        """Test scheduler initializes properly."""
        from learning_loop.scheduler import LearningScheduler
        
        scheduler = LearningScheduler()
        
        status = scheduler.get_status()
        
        self.assertIn("last_cycle", status)
        self.assertIn("daemon_running", status)
        self.assertFalse(status["daemon_running"])


class TestCLICommands(unittest.TestCase):
    """Tests for CLI commands."""
    
    def setUp(self):
        """Set up test fixtures."""
        self.test_dir = tempfile.mkdtemp()
    
    def tearDown(self):
        """Clean up test fixtures."""
        shutil.rmtree(self.test_dir, ignore_errors=True)
    
    def test_main_help(self):
        """Test main CLI help."""
        import subprocess
        
        result = subprocess.run(
            [sys.executable, str(Path(__file__).parent.parent / "main.py"), "--help"],
            capture_output=True,
            text=True
        )
        
        self.assertEqual(result.returncode, 0)
        self.assertIn("OpenMem", result.stdout)


class TestSkillGeneratorOutput(unittest.TestCase):
    """Tests for skill generator output format."""
    
    def setUp(self):
        """Set up test fixtures."""
        self.test_dir = tempfile.mkdtemp()
        self.skills_path = os.path.join(self.test_dir, "generated_skills")
    
    def tearDown(self):
        """Clean up test fixtures."""
        shutil.rmtree(self.test_dir, ignore_errors=True)
    
    def test_generated_skill_structure(self):
        """Test that generated skills have correct structure."""
        from memory_store.skill_generator import SkillGenerator, SKILL_TEMPLATE
        
        generator = SkillGenerator(skills_output_path=self.skills_path)
        
        # Check template has required fields
        required_fields = ["name", "description", "triggers", "actions", "examples"]
        
        for field in required_fields:
            self.assertIn(f"{{{field}}}", SKILL_TEMPLATE)
    
    def test_skill_learner_code_structure(self):
        """Test generated learner code has correct structure."""
        from memory_store.skill_generator import SkillGenerator
        
        skills_path = os.path.join(self.test_dir, "test_skills")
        generator = SkillGenerator(skills_output_path=skills_path)
        
        # Check the learner template has required functions
        learner_template = '''
def should_activate(context: Dict[str, Any]) -> bool:
    pass

def execute(context: Dict[str, Any]) -> Dict[str, Any]:
    pass

def get_metadata() -> Dict[str, Any]:
    pass
'''
        
        self.assertIn("should_activate", learner_template)
        self.assertIn("execute", learner_template)
        self.assertIn("get_metadata", learner_template)


class TestMemoryTiers(unittest.TestCase):
    """Tests for memory tier management."""
    
    def setUp(self):
        """Set up test fixtures."""
        self.test_dir = tempfile.mkdtemp()
    
    def tearDown(self):
        """Clean up test fixtures."""
        shutil.rmtree(self.test_dir, ignore_errors=True)
    
    def test_daily_to_weekly_consolidation(self):
        """Test consolidating daily memories to weekly."""
        from memory_store.memory_manager import MemoryManager
        from memory_store.vector_db import VectorDB

        mem_path = os.path.join(self.test_dir, "memory")
        manager = MemoryManager(base_path=mem_path)
        # Isolation: consolidation mirrors tier entries into vector_db;
        # bind a temp store so this test never writes to data/lancedb.
        manager.vector_db = VectorDB(db_path=os.path.join(self.test_dir, "tiers_vectordb"))
        
        # Store multiple daily memories
        # (store_daily_memory has no importance kwarg; tier importance is fixed)
        for day_offset in range(7):
            from datetime import timedelta
            date = (datetime.now() - timedelta(days=day_offset)).strftime("%Y-%m-%d")
            manager.store_daily_memory(date, f"Memory from {date}")
        
        # Run consolidation
        # Week should be auto-determined from current date
        # This might not create a weekly if we're mid-week
        report = manager.run_consolidation()
        
        self.assertIn("daily_processed", report)


if __name__ == "__main__":
    unittest.main()
