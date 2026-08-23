#!/usr/bin/env python3
"""
Quick test runner for OpenMem.
Run basic tests to verify the system is working.
"""

import os
import sys
from pathlib import Path

# Add openmem to path
OPENMEM_DIR = Path(__file__).parent
sys.path.insert(0, str(OPENMEM_DIR))


def test_imports():
    """Test that all modules can be imported."""
    print("[1/5] Testing imports...")
    
    try:
        from memory_store import VectorDB, get_vector_db
        from memory_store.memory_manager import MemoryManager
        from memory_store.user_model import UserModel
        from memory_store.skill_generator import SkillGenerator
        from learning_loop.conversation_indexer import ConversationIndexer
        from learning_loop.pattern_recognizer import PatternRecognizer
        from learning_loop.reflection_engine import ReflectionEngine
        from learning_loop.scheduler import LearningScheduler
        print("    ✓ All imports successful")
        return True
    except ImportError as e:
        print(f"    ✗ Import failed: {e}")
        return False


def test_basic_functionality():
    """Test basic VectorDB functionality."""
    print("[2/5] Testing basic functionality...")
    
    import tempfile
    from memory_store.vector_db import VectorDB
    
    test_dir = tempfile.mkdtemp()
    db = VectorDB(db_path=os.path.join(test_dir, "test"))
    
    try:
        # Test adding memory
        memory_id = db.add_memory(
            content="Test memory content",
            session_id="test_session",
            importance=0.7
        )
        assert memory_id is not None
        
        # Test searching
        results = db.search("test content")
        assert len(results) > 0
        
        # Test user profile
        db.set_user_profile("test_key", "test_value")
        profile = db.get_user_profile("test_key")
        assert profile["value"] == "test_value"
        
        db.close()
        print("    ✓ Basic functionality works")
        return True
    except Exception as e:
        print(f"    ✗ Functionality test failed: {e}")
        return False
    finally:
        import shutil
        shutil.rmtree(test_dir, ignore_errors=True)


def test_user_model():
    """Test user model functionality."""
    print("[3/5] Testing user model...")
    
    try:
        from memory_store.user_model import UserModel
        
        model = UserModel()
        
        # Test style analysis
        style = model._analyze_style("Hello, how can I help you today?")
        assert "formality" in style
        
        # Test topic extraction
        topics = model._extract_topics("I need help with Python programming")
        assert "python" in topics or "coding" in topics
        
        # Test fact extraction
        facts = model._extract_facts("My name is Alice and I work on AI")
        assert "user_name" in facts
        
        print("    ✓ User model works")
        return True
    except Exception as e:
        print(f"    ✗ User model test failed: {e}")
        return False


def test_pattern_recognition():
    """Test pattern recognition."""
    print("[4/5] Testing pattern recognition...")
    
    try:
        from learning_loop.pattern_recognizer import PatternRecognizer
        
        recognizer = PatternRecognizer()
        
        # Test request classification
        req_type = recognizer._classify_request("Build me a Python web app")
        assert req_type == "build_request"
        
        # Test response strategy classification
        strategy = recognizer._classify_response_strategy("## Step 1\n## Step 2")
        assert strategy == "structured_format"
        
        print("    ✓ Pattern recognition works")
        return True
    except Exception as e:
        print(f"    ✗ Pattern recognition test failed: {e}")
        return False


def test_reflection_engine():
    """Test reflection engine."""
    print("[5/5] Testing reflection engine...")
    
    try:
        from learning_loop.reflection_engine import ReflectionEngine
        from datetime import datetime
        
        engine = ReflectionEngine()
        
        # Test session reflection
        messages = [
            {"role": "user", "content": "Help with Python", "session_id": "test"},
            {"role": "assistant", "content": "Here's Python help", "session_id": "test"},
            {"role": "user", "content": "Perfect thanks!", "session_id": "test"},
        ]
        
        reflection = engine.reflect_on_session(messages)
        assert reflection["outcome"] == "success"
        
        print("    ✓ Reflection engine works")
        return True
    except Exception as e:
        print(f"    ✗ Reflection engine test failed: {e}")
        return False


def main():
    print("=" * 50)
    print("OpenMem Quick Test Suite")
    print("=" * 50)
    print()
    
    results = []
    
    results.append(test_imports())
    results.append(test_basic_functionality())
    results.append(test_user_model())
    results.append(test_pattern_recognition())
    results.append(test_reflection_engine())
    
    print()
    print("=" * 50)
    
    passed = sum(results)
    total = len(results)
    
    if passed == total:
        print(f"✓ All {total} tests passed!")
        print()
        print("OpenMem is ready to use!")
        print()
        print("To get started:")
        print("  python main.py status    # Check system status")
        print("  python main.py run-cycle # Run first learning cycle")
        print("  python main.py setup     # Setup OpenClaw integration")
        return 0
    else:
        print(f"✗ {total - passed}/{total} tests failed")
        print()
        print("Please check the errors above and ensure all dependencies are installed:")
        print("  pip install -r requirements.txt")
        return 1


if __name__ == "__main__":
    sys.exit(main())
