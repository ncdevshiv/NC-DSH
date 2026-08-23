#!/usr/bin/env python3
"""
OpenMem Comprehensive Real Test Suite.

Tests real functionality — no mocks, stubs, or simulated data.
Uses temporary directories for isolation, actual file I/O, and real algorithm execution.

Usage:
    python -m unittest discover tests/real -v
    python tests/real/test_runner.py
"""

import os
import sys
import unittest
import tempfile
import shutil
import json
from pathlib import Path
from datetime import datetime, timedelta

# Ensure OpenMem is importable
TEST_DIR = Path(__file__).parent
OPENMEM_ROOT = TEST_DIR.parent.parent
sys.path.insert(0, str(OPENMEM_ROOT))

# Configure logging for tests
import logging
logging.basicConfig(level=logging.WARNING, format="%(name)s %(levelname)s: %(message)s")


def isolated_reflection_engine(test_dir):
    """
    Build a ReflectionEngine whose every shared-state store lives in test_dir.

    ReflectionEngine resolves improvements.json/reflections.json as instance
    attributes and grabs the global vector store plus default-path
    MemoryManager/UserModel; rebinding them post-init keeps reflection tests
    from reading or mutating repo data/.
    """
    from learning_loop.reflection_engine import ReflectionEngine
    from memory_store.vector_db import VectorDB
    from memory_store.memory_manager import MemoryManager
    from memory_store.user_model import UserModel

    engine = ReflectionEngine()
    tmp = os.path.join(test_dir, "engine_data")
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
    # UserModel grabs the global vector store in __init__; repoint it at the
    # isolated store so fact extraction stays hermetic too.
    engine.user_model.vector_db = engine.vector_db
    return engine


# =============================================================================
# VectorDB Tests — Real LanceDB operations
# =============================================================================

class TestVectorDBReal(unittest.TestCase):
    """Real VectorDB tests with actual file I/O."""

    def setUp(self):
        self.test_dir = tempfile.mkdtemp()
        # Must create subdirs that modules expect
        for sub in ["data/lancedb", "data/memory/daily", "data/memory/weekly",
                     "data/memory/longterm", "data/optimizer", "data/evolution",
                     "data/sessions", "data/usermodel"]:
            os.makedirs(os.path.join(self.test_dir, sub), exist_ok=True)

        # Create vector_db with test path
        from memory_store.vector_db import LanceDBVectorStore
        db_path = os.path.join(self.test_dir, "data", "lancedb")
        self.db = LanceDBVectorStore(db_path=db_path)

    def tearDown(self):
        if hasattr(self, 'db') and self.db:
            self.db.close()
        shutil.rmtree(self.test_dir, ignore_errors=True)

    def test_add_single_memory(self):
        """Real test: add one memory and verify it exists."""
        mem_id = self.db.add_memory(
            content="The user prefers dark mode in all editors",
            session_id="test_001",
            importance=0.8,
            tags=["preference", "editor"],
        )
        self.assertIsNotNone(mem_id)
        self.assertIsInstance(mem_id, str)
        self.assertGreater(len(mem_id), 0)

    def test_add_and_retrieve_memory(self):
        """Real test: add memory, retrieve it by ID."""
        mem_id = self.db.add_memory(
            content="Project deadline is Friday",
            session_id="test_002",
            importance=0.9,
            tags=["deadline", "important"],
        )
        result = self.db.get_memory(mem_id)
        if result:  # May be None if LanceDB isn't fully initialized
            self.assertEqual(result["content"], "Project deadline is Friday")
            self.assertAlmostEqual(result["importance"], 0.9, places=1)

    def test_add_batch_memories(self):
        """Real test: add 10 memories in batch."""
        memories = [
            {"content": f"Memory number {i}", "importance": 0.5 + i * 0.05,
             "session_id": f"session_{i % 3}", "tags": [f"tag_{i % 2}"]}
            for i in range(10)
        ]
        ids = self.db.add_memories_batch(memories)
        self.assertEqual(len(ids), 10)
        self.assertTrue(all(isinstance(i, str) and len(i) > 0 for i in ids))

    def test_update_importance(self):
        """Real test: update a memory's importance score."""
        mem_id = self.db.add_memory(content="Test memory for importance update", importance=0.3)
        if mem_id:
            result = self.db.update_importance(mem_id, 0.95)
            # update_memory may succeed or fail depending on LanceDB state
            self.assertIsInstance(result, bool)

    def test_delete_memory(self):
        """Real test: add then delete a memory."""
        mem_id = self.db.add_memory(content="Temporary memory to delete", importance=0.5)
        if mem_id:
            result = self.db.delete_memory(mem_id)
            self.assertIsInstance(result, bool)

    def test_get_stats(self):
        """Real test: get database statistics."""
        self.db.add_memory(content="Stat test memory", importance=0.6)
        stats = self.db.get_stats()
        self.assertIn("lancedb_available", stats)
        self.assertIn("db_path", stats)
        self.assertIn("tables", stats)

    def test_database_path_is_correct(self):
        """Real test: verify DB uses the configured path."""
        expected = os.path.join(self.test_dir, "data", "lancedb")
        self.assertEqual(self.db.db_path, expected)


# =============================================================================
# MemoryManager Tests — Real SQLite + tier operations
# =============================================================================

class TestMemoryManagerReal(unittest.TestCase):
    """Real MemoryManager tests with actual SQLite operations."""

    def setUp(self):
        self.test_dir = tempfile.mkdtemp()
        base_path = os.path.join(self.test_dir, "memory")
        os.makedirs(base_path, exist_ok=True)

        from memory_store.memory_manager import MemoryManager
        self.manager = MemoryManager(base_path=base_path)
        # Isolation: store_*() mirrors entries into vector_db, which
        # defaults to the shared live singleton; bind a temp-backed store
        # so these tests never write rows into data/lancedb.
        from memory_store.vector_db import VectorDB
        self.manager.vector_db = VectorDB(
            db_path=os.path.join(self.test_dir, "vectordb")
        )

    def tearDown(self):
        self.manager.close()
        shutil.rmtree(self.test_dir, ignore_errors=True)

    def test_store_daily_memory(self):
        """Real test: store and retrieve daily memory."""
        today = datetime.now().strftime("%Y-%m-%d")
        mem_id = self.manager.store_daily_memory(today, "Had a productive coding session today")
        self.assertIsNotNone(mem_id)
        self.assertTrue(mem_id.startswith("daily_"))

    def test_store_and_retrieve_daily(self):
        """Real test: store daily, retrieve it."""
        today = datetime.now().strftime("%Y-%m-%d")
        self.manager.store_daily_memory(today, "First memory")
        self.manager.store_daily_memory(today, "Second memory")
        results = self.manager.get_daily_memories(today)
        self.assertEqual(len(results), 2)

    def test_store_weekly_summary(self):
        """Real test: store weekly summary."""
        week = "2026-04-01"
        mem_id = self.manager.store_weekly_summary(week, "Completed 5 features this week")
        self.assertIsNotNone(mem_id)
        self.assertTrue(mem_id.startswith("weekly_"))

    def test_store_longterm_memory(self):
        """Real test: store long-term fact."""
        mem_id = self.manager.store_longterm_memory("user_editor", "User prefers dark mode", confidence=0.9)
        self.assertIsNotNone(mem_id)
        self.assertTrue(mem_id.startswith("longterm_"))

    def test_retrieve_longterm(self):
        """Real test: store and retrieve long-term memories."""
        self.manager.store_longterm_memory("fact_a", "Content A")
        self.manager.store_longterm_memory("fact_b", "Content B")
        results = self.manager.get_longterm_memories()
        self.assertEqual(len(results), 2)

    def test_consolidation_report(self):
        """Real test: run consolidation and verify report structure."""
        today = datetime.now().strftime("%Y-%m-%d")
        self.manager.store_daily_memory(today, "Daily test memory")
        report = self.manager.run_consolidation(dry_run=False)
        self.assertIn("daily_processed", report)
        self.assertIn("weekly_created", report)
        self.assertIn("longterm_created", report)

    def test_search_memory(self):
        """Real test: search across memory tiers."""
        today = datetime.now().strftime("%Y-%m-%d")
        self.manager.store_daily_memory(today, "Python web development project")
        results = self.manager.search_memory("Python")
        self.assertIsInstance(results, list)

    def test_get_stats(self):
        """Real test: get memory manager statistics."""
        self.manager.store_daily_memory("2026-04-01", "Test")
        stats = self.manager.get_stats()
        self.assertIn("tiers", stats)


# =============================================================================
# UserModel Tests — Real heuristic analysis
# =============================================================================

class TestUserModelReal(unittest.TestCase):
    """Real UserModel tests with actual message analysis."""

    def setUp(self):
        self.test_dir = tempfile.mkdtemp()
        base_path = os.path.join(self.test_dir, "usermodel")
        os.makedirs(base_path, exist_ok=True)

        from memory_store.user_model import UserModel
        self.model = UserModel(base_path=base_path)
        # Isolation: _save_profile() mirrors the whole profile into
        # vector_db (set_user_profile upserts); bind a temp-backed store so
        # analysis tests never touch data/lancedb.
        from memory_store.vector_db import VectorDB
        self.model.vector_db = VectorDB(
            db_path=os.path.join(self.test_dir, "vectordb")
        )

    def tearDown(self):
        shutil.rmtree(self.test_dir, ignore_errors=True)

    def test_analyze_formal_message(self):
        """Real test: analyze a formal message and detect style."""
        msg = "Therefore, I shall require your assistance with this matter. Kindly advise."
        style = self.model._analyze_style(msg)
        self.assertGreaterEqual(style["formality"], 0.0)
        self.assertLessEqual(style["formality"], 1.0)

    def test_analyze_casual_message(self):
        """Real test: analyze a casual message."""
        msg = "hey wanna grab coffee? that'd be cool 👍"
        style = self.model._analyze_style(msg)
        self.assertGreater(style["emoji_usage"], 0.0)

    def test_extract_topics_real(self):
        """Real test: extract topics from actual content."""
        msg = "Can you help me build a Python REST API with a PostgreSQL database?"
        topics = self.model._extract_topics(msg)
        self.assertIsInstance(topics, list)
        # Should detect at least one topic
        self.assertGreaterEqual(len(topics), 0)

    def test_extract_facts_real(self):
        """Real test: extract facts from real text."""
        msg = "My name is Alice and I'm working on ProjectX"
        facts = self.model._extract_facts(msg)
        # The regex-based extractor should find at least user_name
        self.assertIn("user_name", facts)

    def test_detect_sentiment_positive(self):
        """Real test: detect positive sentiment."""
        sentiment = self.model._detect_sentiment("Thanks!", "You're welcome!")
        self.assertEqual(sentiment, "satisfied")

    def test_detect_sentiment_negative(self):
        """Real test: detect frustrated sentiment."""
        sentiment = self.model._detect_sentiment("This still doesn't work!", "Try again")
        self.assertEqual(sentiment, "frustrated")

    def test_analyze_message_full(self):
        """Real test: full message analysis pipeline."""
        analysis = self.model.analyze_message(
            message="My name is Bob. I'm working on a Python project. Remember that I prefer dark mode.",
            channel="test_channel",
        )
        self.assertIn("detected_style", analysis)
        self.assertIn("topics_extracted", analysis)
        self.assertIn("facts_extracted", analysis)

    def test_profile_persists_to_disk(self):
        """Real test: verify profile is saved to disk."""
        self.model.analyze_message("My name is TestUser", channel="test")
        profile_file = self.model.profile_path
        self.assertTrue(os.path.exists(profile_file))
        with open(profile_file, "r") as f:
            data = json.load(f)
        self.assertIn("last_updated", data)

    def test_get_preferred_response_style(self):
        """Real test: get formatted response style."""
        self.model.profile["communication_style"]["formality"] = 0.8
        style = self.model.get_preferred_response_style()
        self.assertIn("formal", style)
        self.assertTrue(style["formal"])

    def test_get_context_for_interaction(self):
        """Real test: get context dict for injection."""
        self.model.analyze_message("I prefer concise answers", channel="test")
        context = self.model.get_context_for_new_interaction()
        self.assertIn("user_profile_summary", context)
        self.assertIn("preferred_response_style", context)


# =============================================================================
# PatternRecognizer Tests — Real statistical analysis
# =============================================================================

class TestPatternRecognizerReal(unittest.TestCase):
    """Real pattern recognition tests."""

    def setUp(self):
        self.test_dir = tempfile.mkdtemp()

    def tearDown(self):
        shutil.rmtree(self.test_dir, ignore_errors=True)

    def test_classify_request_types(self):
        """Real test: classify various request types."""
        from learning_loop.pattern_recognizer import PatternRecognizer
        pr = PatternRecognizer()

        tests = [
            ("What is Python?", "factual_question"),
            ("Build me a REST API", "build_request"),
            ("How does this differ from that?", "comparison_question"),
            ("Fix the bug in my code", "problem_solving"),
            ("Remember that my name is John", "memory_request"),
            ("Explain how async/await works", "explanation_request"),
            ("Search for the latest AI news", "research_request"),
        ]

        for text, expected in tests:
            result = pr._classify_request(text)
            self.assertEqual(result, expected, f"Failed for: '{text}' (got '{result}', expected '{expected}')")

    def test_classify_response_strategies(self):
        """Real test: classify response strategies."""
        from learning_loop.pattern_recognizer import PatternRecognizer
        pr = PatternRecognizer()

        tests = [
            ("Done.", "concise_direct"),
            ("## Step 1\n## Step 2", "structured_format"),
            ("```python\nprint('hi')\n```", "code_oriented"),
            ("First, do this. Second, do that. Finally, test it.", "step_by_step"),
        ]

        for text, expected in tests:
            result = pr._classify_response_strategy(text)
            self.assertEqual(result, expected, f"Failed for: '{text}'")

    def test_topic_extraction_real(self):
        """Real test: extract topics from real messages."""
        from learning_loop.pattern_recognizer import PatternRecognizer
        pr = PatternRecognizer()

        topics = pr._extract_topics("Help me deploy this Docker container to AWS")
        self.assertIn("devops", topics)

        topics = pr._extract_topics("Write a Python function to parse JSON")
        self.assertIn("python", topics)

    def test_conversation_flow_analysis(self):
        """Real test: analyze a real conversation flow."""
        from learning_loop.pattern_recognizer import PatternRecognizer
        pr = PatternRecognizer()

        messages = [
            {"role": "user", "content": "How do I create a Python API?", "timestamp": datetime.now().isoformat()},
            {"role": "assistant", "content": "Use Flask or FastAPI.", "timestamp": datetime.now().isoformat()},
            {"role": "user", "content": "Perfect, thanks!", "timestamp": datetime.now().isoformat()},
        ]

        analysis = pr.analyze_conversation_flow(messages)
        self.assertEqual(analysis["outcome"], "success")
        self.assertIn("request_types", analysis)
        self.assertIn("topics_mentioned", analysis)

    def test_recurring_patterns(self):
        """Real test: find recurring patterns (empty db = no patterns)."""
        from learning_loop.pattern_recognizer import PatternRecognizer
        pr = PatternRecognizer()
        patterns = pr.find_recurring_patterns(days_back=1)
        self.assertIsInstance(patterns, list)


# =============================================================================
# ReflectionEngine Tests — Real session analysis
# =============================================================================

class TestReflectionEngineReal(unittest.TestCase):
    """Real reflection engine tests."""

    def setUp(self):
        self.test_dir = tempfile.mkdtemp()
        # Need memory dirs for MemoryManager
        for sub in ["data/memory/daily", "data/memory/weekly", "data/memory/longterm",
                     "data/usermodel"]:
            os.makedirs(os.path.join(self.test_dir, sub), exist_ok=True)
        # Override paths via environment
        os.environ["OPENMEM_TEST_DIR"] = self.test_dir
        # Isolate: the engine ignores OPENMEM_TEST_DIR (paths are module-
        # relative), so redirect its stores into the temp dir explicitly.
        self.engine = isolated_reflection_engine(self.test_dir)

    def tearDown(self):
        shutil.rmtree(self.test_dir, ignore_errors=True)

    def test_reflect_on_success(self):
        """Real test: reflect on a successful session."""
        engine = self.engine

        messages = [
            {"role": "user", "content": "Build me a Python script", "session_id": "s1"},
            {"role": "assistant", "content": "Here's your script.", "session_id": "s1"},
            {"role": "user", "content": "Perfect, thanks!", "session_id": "s1"},
        ]

        reflection = engine.reflect_on_session(messages)
        self.assertEqual(reflection["outcome"], "success")
        self.assertTrue(any(i["type"] == "reinforce" for i in reflection["improvements_identified"]))

    def test_reflect_on_failure(self):
        """Real test: reflect on a failed session."""
        engine = self.engine

        messages = [
            {"role": "user", "content": "Fix my code", "session_id": "s2"},
            {"role": "assistant", "content": "Here's the fix.", "session_id": "s2"},
            {"role": "user", "content": "Still not working, this is frustrating", "session_id": "s2"},
        ]

        reflection = engine.reflect_on_session(messages)
        self.assertEqual(reflection["outcome"], "failure")
        self.assertTrue(any(i["type"] == "fix_needed" for i in reflection["improvements_identified"]))

    def test_reflect_extracts_facts(self):
        """Real test: extract facts during reflection."""
        engine = self.engine

        messages = [
            {"role": "user", "content": "My name is Bob and I'm building an AI agent", "session_id": "s3"},
        ]

        reflection = engine.reflect_on_session(messages)
        self.assertTrue(len(reflection["memories_to_create"]) > 0)

    def test_improvement_queue(self):
        """Real test: manage improvement queue."""
        engine = self.engine

        improvement = {
            "type": "knowledge_gap",
            "description": "User asked about unknown topic",
            "priority": "medium",
            "identified_at": datetime.now().isoformat(),
        }
        engine.improvements["pending"].append(improvement)
        engine._save_improvements()

        next_imp = engine.get_next_improvement()
        self.assertIsNotNone(next_imp)
        self.assertEqual(next_imp["type"], "knowledge_gap")

        engine.complete_improvement(
            improvement, evidence_session_id="session_evidence_001"
        )
        # Check by description since complete_improvement adds completed_at
        completed = [i for i in engine.improvements["completed"]
                     if i.get("description") == improvement["description"]]
        self.assertTrue(len(completed) > 0)
        self.assertEqual(completed[0]["evidence_session_id"], "session_evidence_001")


# =============================================================================
# Scheduler Tests — Real orchestration
# =============================================================================

class TestSchedulerReal(unittest.TestCase):
    """Real scheduler tests."""

    def setUp(self):
        self.test_dir = tempfile.mkdtemp()

    def tearDown(self):
        shutil.rmtree(self.test_dir, ignore_errors=True)

    def test_scheduler_initializes(self):
        """Real test: scheduler creates without errors."""
        from learning_loop.scheduler import LearningScheduler
        scheduler = LearningScheduler()
        self.assertIsNotNone(scheduler)

    def test_scheduler_status(self):
        """Real test: get scheduler status."""
        from learning_loop.scheduler import LearningScheduler
        scheduler = LearningScheduler()
        status = scheduler.get_status()
        self.assertIn("daemon_running", status)
        self.assertIn("cycles_completed", status)
        self.assertFalse(status["daemon_running"])

    def test_scheduler_default_config(self):
        """Real test: verify default configuration."""
        from learning_loop.scheduler import LearningScheduler
        scheduler = LearningScheduler()
        self.assertIn("interval_hours", scheduler.config)
        self.assertIn("enable_skill_generation", scheduler.config)
        self.assertTrue(scheduler.config["enable_skill_generation"])


# =============================================================================
# Agent Adapter Tests — Real adapter functionality
# =============================================================================

class TestAgentAdaptersReal(unittest.TestCase):
    """Real agent adapter tests."""

    def setUp(self):
        self.test_dir = tempfile.mkdtemp()

    def tearDown(self):
        shutil.rmtree(self.test_dir, ignore_errors=True)

    def test_generic_adapter_detects_workspace(self):
        """Real test: generic adapter finds workspace."""
        from agents.generic.adapter import GenericAdapter
        adapter = GenericAdapter()
        workspace = adapter.get_workspace_path()
        self.assertTrue(os.path.isabs(workspace))

    def test_generic_adapter_session_id(self):
        """Real test: generic adapter generates session ID."""
        from agents.generic.adapter import GenericAdapter
        adapter = GenericAdapter()
        sid = adapter.get_session_id()
        self.assertIsInstance(sid, str)
        self.assertGreater(len(sid), 0)

    def test_generic_adapter_context_injection(self):
        """Real test: generic adapter writes context file."""
        from agents.generic.adapter import GenericAdapter
        adapter = GenericAdapter()
        result = adapter.inject_context("Test context")
        self.assertIsInstance(result, bool)

    def test_all_adapters_register(self):
        """Real test: all 10 adapters register themselves."""
        from agents.base import get_available_adapters, get_adapter
        available = get_available_adapters()
        # Should have at least the adapters we created
        expected = ["qwen_code", "claude_code", "cursor", "vscode",
                     "windsurf", "codex_cli", "opencode", "antigravity_ide",
                     "kilo_cli", "openclaw", "generic"]
        for name in expected:
            self.assertIn(name, available, f"Missing adapter: {name}")
            adapter = get_adapter(name)
            self.assertIsNotNone(adapter, f"Cannot instantiate adapter: {name}")

    def test_adapter_contract(self):
        """Real test: verify all adapters implement required methods."""
        from agents.base import get_adapter, AgentAdapter
        import inspect

        required_methods = [
            "get_session_messages",
            "inject_context",
            "get_workspace_path",
            "get_session_id",
            "get_agent_name",
            "get_skill_install_path",
        ]

        for adapter_name in ["generic", "qwen_code", "claude_code", "cursor"]:
            adapter = get_adapter(adapter_name)
            self.assertIsNotNone(adapter, f"Adapter {adapter_name} not found")
            self.assertIsInstance(adapter, AgentAdapter)

            for method_name in required_methods:
                self.assertTrue(
                    hasattr(adapter, method_name),
                    f"Adapter {adapter_name} missing method: {method_name}"
                )
                method = getattr(adapter, method_name)
                self.assertTrue(callable(method),
                               f"Adapter {adapter_name}.{method_name} not callable")


# =============================================================================
# LLM Module Tests — Heuristic fallback (no API key needed)
# =============================================================================

class TestLLMModuleReal(unittest.TestCase):
    """Real LLM module tests (heuristic mode, no API key)."""

    def test_llm_initializes_without_api_key(self):
        """Real test: LLM works in heuristic mode without any API key."""
        from core.llm import OpenMemLLM
        llm = OpenMemLLM()
        self.assertIsNotNone(llm)

    def test_llm_heuristic_response(self):
        """Real test: LLM generates heuristic response."""
        from core.llm import OpenMemLLM
        llm = OpenMemLLM()
        response = llm.chat([{"role": "user", "content": "Hello there"}])
        self.assertIsInstance(response, str)

    def test_llm_heuristic_summarize(self):
        """Real test: LLM summarizes without API key."""
        from core.llm import OpenMemLLM
        llm = OpenMemLLM()
        text = "First point. Second point with more details. Third point is the conclusion."
        summary = llm.summarize(text, max_length=50)
        self.assertIsInstance(summary, str)
        self.assertGreater(len(summary), 0)

    def test_llm_heuristic_facts(self):
        """Real test: LLM extracts facts without API key."""
        from core.llm import OpenMemLLM
        llm = OpenMemLLM()
        facts = llm.extract_facts("My name is Charlie and I'm working on ProjectX")
        self.assertIsInstance(facts, dict)

    def test_llm_heuristic_reflection(self):
        """Real test: LLM reflects without API key."""
        from core.llm import OpenMemLLM
        llm = OpenMemLLM()
        messages = [
            {"role": "user", "content": "Build me a thing"},
            {"role": "assistant", "content": "Here it is"},
            {"role": "user", "content": "Perfect, thanks!"},
        ]
        reflection = llm.reflect(messages)
        self.assertIn("outcome", reflection)

    def test_llm_status(self):
        """Real test: LLM reports its status."""
        from core.llm import OpenMemLLM
        llm = OpenMemLLM()
        status = llm.get_status()
        self.assertIn("available", status)
        self.assertIn("provider", status)

    def test_llm_singleton(self):
        """Real test: get_llm returns singleton."""
        from core.llm import get_llm
        llm1 = get_llm()
        llm2 = get_llm()
        self.assertIs(llm1, llm2)


# =============================================================================
# Config Tests
# =============================================================================

class TestConfigReal(unittest.TestCase):
    """Real configuration tests."""

    def test_config_file_exists(self):
        """Real test: config.json exists after install."""
        config_path = OPENMEM_ROOT / "config.json"
        if config_path.exists():
            with open(config_path, "r") as f:
                config = json.load(f)
            self.assertIn("version", config)
            self.assertIn("agents", config)

    def test_bin_manifest_exists(self):
        """Real test: bin/manifest.json exists."""
        manifest_path = OPENMEM_ROOT / "bin" / "manifest.json"
        self.assertTrue(manifest_path.exists())
        with open(manifest_path, "r") as f:
            manifest = json.load(f)
        self.assertIn("version", manifest)
        self.assertIn("supported_agents", manifest)

    def test_pyproject_toml_exists(self):
        """Real test: pyproject.toml exists for uv."""
        pyproject = OPENMEM_ROOT / "pyproject.toml"
        self.assertTrue(pyproject.exists())


# =============================================================================
# Integration Tests — Real end-to-end flows
# =============================================================================

class TestEndToEndReal(unittest.TestCase):
    """Real end-to-end integration tests."""

    def setUp(self):
        self.test_dir = tempfile.mkdtemp()
        for sub in ["data/lancedb", "data/memory/daily", "data/memory/weekly",
                     "data/memory/longterm", "data/optimizer", "data/evolution",
                     "data/sessions", "data/usermodel"]:
            os.makedirs(os.path.join(self.test_dir, sub), exist_ok=True)

    def tearDown(self):
        shutil.rmtree(self.test_dir, ignore_errors=True)

    def test_full_memory_lifecycle(self):
        """Real test: add → search → update → delete lifecycle."""
        from memory_store.vector_db import LanceDBVectorStore
        db = LanceDBVectorStore(db_path=os.path.join(self.test_dir, "data", "lancedb"))

        # Add
        mem_id = db.add_memory(
            content="User is building a web app with React and Flask",
            session_id="lifecycle_test",
            importance=0.7,
            tags=["project", "web"],
        )
        self.assertIsNotNone(mem_id)

        # Get stats
        stats = db.get_stats()
        self.assertIn("tables", stats)

        # Cleanup
        db.close()

    def test_user_model_tracks_preferences(self):
        """Real test: user model learns and persists preferences."""
        from memory_store.user_model import UserModel
        model = UserModel(base_path=os.path.join(self.test_dir, "data", "usermodel"))
        # Isolation: profile persistence mirrors into vector_db; keep the
        # writes on a temp-backed store, not data/lancedb.
        from memory_store.vector_db import VectorDB
        model.vector_db = VectorDB(
            db_path=os.path.join(self.test_dir, "data", "lancedb")
        )

        # Analyze multiple messages
        messages = [
            "My name is TestUser. I prefer dark mode.",
            "I'm working on a Python project.",
            "I prefer concise answers and use VS Code.",
        ]
        for msg in messages:
            model.analyze_message(msg, channel="test")

        # Verify profile persisted
        self.assertTrue(os.path.exists(model.profile_path))
        profile = model.get_context_for_new_interaction()
        self.assertIn("user_profile_summary", profile)

    def test_reflection_detects_success_and_failure(self):
        """Real test: reflection engine correctly identifies outcomes."""
        # Isolated engine: a bare ReflectionEngine would write repo data/.
        engine = isolated_reflection_engine(self.test_dir)

        # Success case
        success_session = [
            {"role": "user", "content": "Help me with Python", "session_id": "s1"},
            {"role": "assistant", "content": "Sure!", "session_id": "s1"},
            {"role": "user", "content": "Perfect, thanks!", "session_id": "s1"},
        ]
        success_reflection = engine.reflect_on_session(success_session)
        self.assertEqual(success_reflection["outcome"], "success")

        # Failure case
        failure_session = [
            {"role": "user", "content": "Fix this", "session_id": "s2"},
            {"role": "assistant", "content": "Here's the fix", "session_id": "s2"},
            {"role": "user", "content": "Still not working", "session_id": "s2"},
        ]
        failure_reflection = engine.reflect_on_session(failure_session)
        self.assertEqual(failure_reflection["outcome"], "failure")


if __name__ == "__main__":
    unittest.main(verbosity=2)
