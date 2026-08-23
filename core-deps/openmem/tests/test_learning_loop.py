"""
Tests for OpenMem learning_loop module.
"""

import unittest
import os
import sys
import tempfile
import shutil
import json
from pathlib import Path
from datetime import datetime, timedelta

# Add parent to path
sys.path.insert(0, str(Path(__file__).parent.parent))

from memory_store.vector_db import VectorDB
from memory_store.memory_manager import MemoryManager
from memory_store.user_model import UserModel
from learning_loop.conversation_indexer import ConversationIndexer
from learning_loop.pattern_recognizer import PatternRecognizer
from learning_loop.reflection_engine import ReflectionEngine


class TestConversationIndexer(unittest.TestCase):
    """Tests for ConversationIndexer."""
    
    def setUp(self):
        """Set up test fixtures."""
        self.test_dir = tempfile.mkdtemp()
        
        # Create mock workspace structure
        self.workspace = os.path.join(self.test_dir, "workspace")
        os.makedirs(self.workspace)
        
        self.indexer = ConversationIndexer(openclaw_workspace=self.workspace)
    
    def tearDown(self):
        """Clean up test fixtures."""
        shutil.rmtree(self.test_dir, ignore_errors=True)
    
    def test_score_message_importance(self):
        """Test importance scoring."""
        messages = [
            {"content": "Thanks, that works perfectly!", "role": "user"},
            {"content": "Remember to fix the bug in the API", "role": "user"},
            {"content": "Hey what's up", "role": "user"},
        ]
        
        scores = [self.indexer.score_message_importance(m) for m in messages]
        
        # Success message should be high
        self.assertGreater(scores[0], 0.5)
        # Memory trigger should be high
        self.assertGreater(scores[1], 0.5)
        # Casual should be lower
        self.assertLess(scores[2], 0.6)
    
    def test_extract_tags(self):
        """Test tag extraction."""
        messages = [
            {"content": "Can you help me with my Python code?", "channel": "telegram"},
            {"content": "Search the web for latest AI news", "channel": "discord"},
            {"content": "Build me a web app with React", "channel": "whatsapp"},
        ]
        
        tags = [self.indexer.extract_tags(m) for m in messages]
        
        self.assertTrue(any("python" in t or "coding" in t for t in tags[0]))
        self.assertIn("channel:telegram", tags[0])
        self.assertTrue(any("ai" in t or "research" in t for t in tags[1]))
        self.assertIn("channel:discord", tags[1])
    
    def test_parse_session_messages(self):
        """Test session message parsing."""
        session_data = {
            "id": "test_session",
            "channel": "telegram",
            "messages": [
                {"role": "user", "content": "Hello", "timestamp": "2026-04-01T10:00:00"},
                {"role": "assistant", "content": "Hi there!", "timestamp": "2026-04-01T10:00:05"},
            ]
        }
        
        messages = self.indexer.parse_session_messages(session_data)
        
        self.assertEqual(len(messages), 2)
        self.assertEqual(messages[0]["role"], "user")
        self.assertEqual(messages[0]["content"], "Hello")
        self.assertEqual(messages[1]["role"], "assistant")


class TestPatternRecognizer(unittest.TestCase):
    """Tests for PatternRecognizer."""
    
    def setUp(self):
        """Set up test fixtures."""
        self.test_dir = tempfile.mkdtemp()
        self.recognizer = PatternRecognizer()
    
    def tearDown(self):
        """Clean up test fixtures."""
        shutil.rmtree(self.test_dir, ignore_errors=True)
    
    def test_classify_request(self):
        """Test request classification."""
        requests = [
            ("What is Python?", "factual_question"),
            ("Build me a REST API", "build_request"),
            ("How does this differ from that?", "comparison_question"),
            ("Fix the bug in my code", "problem_solving"),
            ("Remember that my name is John", "memory_request"),
        ]
        
        for text, expected_type in requests:
            result = self.recognizer._classify_request(text)
            self.assertEqual(result, expected_type, f"Failed for: {text}")
    
    def test_classify_response_strategy(self):
        """Test response strategy classification."""
        responses = [
            ("Here's the answer.", "concise_direct"),
            ("## Step 1\n## Step 2\n## Step 3", "structured_format"),
            ("```python\nprint('hello')\n```", "code_oriented"),
        ]
        
        for text, expected_strategy in responses:
            result = self.recognizer._classify_response_strategy(text)
            self.assertEqual(result, expected_strategy)
    
    def test_extract_topics(self):
        """Test topic extraction."""
        contents = [
            "Help me write some Python code for an API",
            "Deploy this Docker container to AWS",
            "What is the difference between SQL and NoSQL?",
        ]
        
        topics = [self.recognizer._extract_topics(c) for c in contents]
        
        self.assertTrue(any(t in ["python", "coding"] for t in topics[0]))
        self.assertTrue(any(t in ["devops", "databases"] for t in topics[1]))
        self.assertTrue(any(t in ["databases"] for t in topics[2]))
    
    def test_analyze_conversation_flow(self):
        """Test conversation flow analysis."""
        messages = [
            {"role": "user", "content": "How do I use Python APIs?", "timestamp": datetime.now().isoformat()},
            {"role": "assistant", "content": "Here's how to use Python APIs...", "timestamp": datetime.now().isoformat()},
            {"role": "user", "content": "Thanks, that works perfectly!", "timestamp": datetime.now().isoformat()},
        ]
        
        analysis = self.recognizer.analyze_conversation_flow(messages)
        
        self.assertEqual(analysis["outcome"], "success")
        self.assertIn("factual_question", analysis["request_types"])
        self.assertIn("topics_mentioned", analysis)


class TestReflectionEngine(unittest.TestCase):
    """Tests for ReflectionEngine."""

    def setUp(self):
        """Set up test fixtures."""
        self.test_dir = tempfile.mkdtemp()
        self.engine = ReflectionEngine()
        # Isolation: ReflectionEngine resolves its JSON paths as instance
        # attributes, so rebinding them post-init redirects every
        # _save_reflections/_save_improvements call into the temp dir.
        # State dicts are reset so nothing leaks in from repo data/, and
        # the collaborators are repointed at temp-backed instances so
        # reflection runs never touch data/lancedb or data/memory either.
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
        # UserModel grabs the global vector store in __init__; repoint it at
        # the isolated store so fact extraction stays hermetic too.
        self.engine.user_model.vector_db = self.engine.vector_db

    def tearDown(self):
        """Clean up test fixtures."""
        try:
            self.engine.vector_db.close()
            self.engine.memory_manager.close()
        except Exception:
            pass
        shutil.rmtree(self.test_dir, ignore_errors=True)
    
    def test_extract_facts(self):
        """Test fact extraction."""
        messages = [
            "My name is Alice and I'm working on Project X",
            "I prefer dark mode and use VS Code",
        ]
        
        facts = [self.engine._extract_facts(m) for m in messages]
        
        self.assertIn("user_name", facts[0])
        self.assertIn("current_project", facts[0])
    
    def test_reflect_on_session_success(self):
        """Test reflection on successful session."""
        messages = [
            {"role": "user", "content": "Build me a Python script", "session_id": "test_1"},
            {"role": "assistant", "content": "Here's your Python script...", "session_id": "test_1"},
            {"role": "user", "content": "Perfect, thanks!", "session_id": "test_1"},
        ]
        
        reflection = self.engine.reflect_on_session(messages)
        
        self.assertEqual(reflection["outcome"], "success")
        self.assertTrue(any(i["type"] == "reinforce" for i in reflection["improvements_identified"]))
    
    def test_reflect_on_session_failure(self):
        """Test reflection on failed session."""
        messages = [
            {"role": "user", "content": "Fix my code", "session_id": "test_2"},
            {"role": "assistant", "content": "Here's the fix...", "session_id": "test_2"},
            {"role": "user", "content": "Still not working, this is frustrating", "session_id": "test_2"},
        ]
        
        reflection = self.engine.reflect_on_session(messages)
        
        self.assertEqual(reflection["outcome"], "failure")
        self.assertTrue(any(i["type"] == "fix_needed" for i in reflection["improvements_identified"]))
    
    def test_reflect_on_session_with_facts(self):
        """Test fact extraction in reflection."""
        messages = [
            {"role": "user", "content": "My name is Bob and I'm building an AI agent", "session_id": "test_3"},
            {"role": "assistant", "content": "Nice to meet you Bob!", "session_id": "test_3"},
        ]
        
        reflection = self.engine.reflect_on_session(messages)
        
        self.assertTrue(len(reflection["memories_to_create"]) > 0)
    
    def test_get_next_improvement(self):
        """Test getting next improvement."""
        # Add a pending improvement
        self.engine.improvements["pending"].append({
            "type": "knowledge_gap",
            "description": "User asked about unknown topic",
            "priority": "medium",
            "identified_at": datetime.now().isoformat()
        })
        self.engine._save_improvements()
        
        improvement = self.engine.get_next_improvement()
        
        self.assertIsNotNone(improvement)
        self.assertEqual(improvement["type"], "knowledge_gap")
    
    def test_complete_improvement(self):
        """Test completing an improvement (evidence now required)."""
        improvement = {
            "type": "test",
            "description": "Test improvement"
        }
        self.engine.improvements["pending"].append(dict(improvement))

        self.engine.complete_improvement(
            improvement, evidence_memory_id="mem_evidence_123"
        )

        # complete_improvement stores {**improvement, "completed_at",
        # evidence...}, i.e. a NEW dict, so identity/equality with the
        # original can never hold; assert on the matching member instead.
        completed = [
            c for c in self.engine.improvements["completed"]
            if c.get("type") == improvement["type"]
            and c.get("description") == improvement["description"]
        ]
        self.assertEqual(
            len(completed), 1,
            "exactly one matching completed improvement expected"
        )
        self.assertNotIn(improvement, self.engine.improvements["pending"])
        self.assertIn("completed_at", completed[0])
        self.assertEqual(completed[0]["evidence_memory_id"], "mem_evidence_123")


class TestUserModel(unittest.TestCase):
    """Tests for UserModel."""
    
    def setUp(self):
        """Set up test fixtures."""
        self.test_dir = tempfile.mkdtemp()
        # Hermetic profile store: without this, UserModel reads (and would
        # persist to) the live data/usermodel/ profile, making style tests
        # depend on whatever previous runs left behind.
        self.model = UserModel(base_path=os.path.join(self.test_dir, "usermodel"))
    
    def tearDown(self):
        """Clean up test fixtures."""
        shutil.rmtree(self.test_dir, ignore_errors=True)
    
    def test_analyze_style_formal(self):
        """Test formal style detection."""
        message = "Therefore, I shall require your assistance with this matter. Kindly advise accordingly."
        
        style = self.model._analyze_style(message)
        
        self.assertGreater(style["formality"], 0.5)
    
    def test_analyze_style_casual(self):
        """Test casual style detection."""
        message = "Hey! Wanna grab coffee later? That'd be cool 👍"
        
        style = self.model._analyze_style(message)
        
        self.assertLess(style["formality"], 0.6)
        self.assertGreater(style["emoji_usage"], 0.3)
    
    def test_extract_topics(self):
        """Test topic extraction."""
        message = "Can you help me build a Python web app with React frontend?"

        topics = self.model._extract_topics(message)

        # _extract_topics returns CATEGORY labels ("coding", "web"), not the
        # literal keywords ("python", "react") that triggered them.
        self.assertIn("coding", topics)
        self.assertIn("web", topics)

    def test_extract_facts(self):
        """Test fact extraction."""
        messages = [
            "My name is Charlie and I'm working on Project Alpha",
            "I work at TechCorp and I'm based in Mumbai",
        ]

        facts = [self.model._extract_facts(m) for m in messages]

        # _extract_facts capitalizes extracted names deliberately.
        self.assertEqual(facts[0].get("user_name"), "Charlie")
        self.assertEqual(facts[0].get("current_project"), "alpha")
        self.assertEqual(facts[1].get("company"), "techcorp")
    
    def test_detect_sentiment_positive(self):
        """Test positive sentiment detection."""
        sentiment = self.model._detect_sentiment("Thanks!", "You're welcome!")
        
        self.assertEqual(sentiment, "satisfied")
    
    def test_detect_sentiment_frustrated(self):
        """Test frustrated sentiment detection."""
        sentiment = self.model._detect_sentiment("This still doesn't work!", "Try again")
        
        self.assertEqual(sentiment, "frustrated")
    
    def test_preferred_response_style(self):
        """Test preferred response style generation."""
        # Set up some profile data; emoji_usage must be pinned explicitly
        # because use_emoji derives solely from it (neutral default is 0.5,
        # which maps to emojis ON).
        self.model.profile["communication_style"]["formality"] = 0.8
        self.model.profile["communication_style"]["verbosity"] = 0.3
        self.model.profile["communication_style"]["emoji_usage"] = 0.1

        style = self.model.get_preferred_response_style()

        self.assertTrue(style["formal"])
        self.assertFalse(style["use_emoji"])


if __name__ == "__main__":
    unittest.main()
