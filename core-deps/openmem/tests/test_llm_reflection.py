"""
Gate M2 — LLM reflection wiring tests (mocked at the litellm boundary).

Covers three regimes WITHOUT any real network access (no API keys exist on
this machine; litellm is an optional extra and may be absent):

1. Valid JSON from the LLM boundary -> parsed reflection stored with
   mode="llm" and facts extracted into memory + user model.
2. Malformed JSON / unrecognized shape -> visible logged fallback to
   heuristic mode, cycle continues (mode="heuristic").
3. No-key environment -> availability check performs ZERO network attempts,
   every reflection runs in heuristic mode.

The litellm boundary is mocked by installing a fake module into
sys.modules BEFORE constructing OpenMemLLM, so the same code paths run as
with the real SDK — including the lazy "first completion is the only
network touch" contract asserted here.
"""

import unittest
import os
import sys
import json
import types
import shutil
import tempfile
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).parent.parent))

VALID_REFLECTION_JSON = json.dumps({
    "outcome": "success",
    "what_went_well": ["Clear step-by-step answer"],
    "what_to_improve": [],
    "facts_to_remember": {"user_name": "Alice"},
    "knowledge_gaps": [],
})

MALFORMED_REFLECTION_TEXT = (
    "Sure! The session went really well overall, the user was happy."
)

UNRECOGNIZED_SHAPE_JSON = json.dumps({"weather": "sunny", "notes": [1, 2, 3]})


# ---------------------------------------------------------------------------
# Fake litellm boundary
# ---------------------------------------------------------------------------

class _FakeMessage:
    def __init__(self, content):
        self.content = content


class _FakeChoice:
    def __init__(self, content):
        self.message = _FakeMessage(content)


class _FakeResponse:
    def __init__(self, content):
        self.choices = [_FakeChoice(content)]


def _install_fake_litellm(canned_content=None, raise_error=None):
    """
    Install a fake `litellm` module whose completion() records every call.

    Returns (module, calls) where calls is the shared list of invocation
    records — assertions read it directly to prove network attempts did or
    did not happen.
    """
    calls = []

    def completion(model=None, messages=None, **kwargs):
        calls.append({"model": model, "messages": messages})
        if raise_error is not None:
            raise raise_error
        return _FakeResponse(canned_content)

    mod = types.ModuleType("litellm")
    mod.completion = completion
    mod.openai_key = None
    mod.anthropic_key = None
    mod.google_ai_studio_key = None
    sys.modules["litellm"] = mod
    return mod, calls


def _reset_core_llm_singleton():
    """get_llm() caches process-wide; tests must start from a clean slate."""
    import core.llm as llm_mod
    llm_mod._llm_instance = None


HOSTED_KEY_VARS = ["OPENAI_API_KEY", "ANTHROPIC_API_KEY",
                   "GEMINI_API_KEY", "GOOGLE_API_KEY", "OLLAMA_BASE_URL"]


class LLMBoundaryTestCase(unittest.TestCase):
    """Shared env/singleton hygiene for boundary-mocked tests."""

    def setUp(self):
        _reset_core_llm_singleton()
        # Deterministic no-key baseline; individual tests re-add what they
        # simulate via patch.dict contexts of their own.
        env_patcher = mock.patch.dict(
            os.environ,
            {k: "" for k in HOSTED_KEY_VARS},
        )
        env_patcher.start()
        self.addCleanup(env_patcher.stop)
        self.addCleanup(_reset_core_llm_singleton)

    def make_engine(self):
        """Isolated ReflectionEngine (no repo data/ writes) — same DI shape
        as the fixtures in tests/test_learning_loop.py."""
        from learning_loop.reflection_engine import ReflectionEngine
        from memory_store.vector_db import VectorDB
        from memory_store.memory_manager import MemoryManager
        from memory_store.user_model import UserModel

        self.test_dir = tempfile.mkdtemp(prefix="openmem_llm_refl_")
        tmp = os.path.join(self.test_dir, "engine_data")
        os.makedirs(tmp, exist_ok=True)
        engine = ReflectionEngine()
        engine.reflection_log = os.path.join(tmp, "reflections.json")
        engine.improvements_file = os.path.join(tmp, "improvements.json")
        engine.reflections = {
            "session_reflections": [], "cross_session_reflections": [],
            "corrections_made": [], "last_reflection": None,
        }
        engine.improvements = {"pending": [], "completed": [], "rejected": []}
        engine.vector_db = VectorDB(db_path=os.path.join(tmp, "vectordb"))
        engine.memory_manager = MemoryManager(base_path=os.path.join(tmp, "memory"))
        engine.user_model = UserModel(base_path=os.path.join(tmp, "usermodel"))
        engine.user_model.vector_db = engine.vector_db
        self.addCleanup(shutil.rmtree, self.test_dir, ignore_errors=True)
        return engine


SESSION_MESSAGES = [
    {"role": "user", "content": "My name is Alice and I'm working on Orion",
     "session_id": "llm_t"},
    {"role": "assistant", "content": "Great to meet you, Alice!", "session_id": "llm_t"},
    {"role": "user", "content": "Perfect, thanks!", "session_id": "llm_t"},
]


# ---------------------------------------------------------------------------
# Regime 1: valid JSON -> mode="llm", facts extracted
# ---------------------------------------------------------------------------

class TestValidLLMReflection(LLMBoundaryTestCase):

    def test_openmem_llm_parses_canned_valid_json_without_init_network(self):
        """With a key present, init stays offline and the FIRST completion
        happens only when reflect() is actually used."""
        _, calls = _install_fake_litellm(canned_content=VALID_REFLECTION_JSON)
        self.addCleanup(sys.modules.pop, "litellm", None)

        with mock.patch.dict(os.environ, {"OPENAI_API_KEY": "sk-test-canned-key"}):
            from core.llm import OpenMemLLM
            llm = OpenMemLLM()

            self.assertTrue(llm.is_available)
            self.assertEqual(calls, [], "init must not attempt any network call")

            result = llm.reflect(SESSION_MESSAGES)
            self.assertEqual(result["outcome"], "success")
            self.assertEqual(len(calls), 1, "exactly one completion expected")

    def test_engine_stores_llm_reflection_with_mode_and_facts(self):
        _, calls = _install_fake_litellm(canned_content=VALID_REFLECTION_JSON)
        self.addCleanup(sys.modules.pop, "litellm", None)
        engine = self.make_engine()

        with mock.patch.dict(os.environ, {"OPENAI_API_KEY": "sk-test-canned-key"}):
            from core.llm import OpenMemLLM
            real_llm = OpenMemLLM()
            with mock.patch("core.llm.get_llm", return_value=real_llm):
                reflection = engine.reflect_on_session(SESSION_MESSAGES)

        self.assertEqual(reflection["mode"], "llm")
        self.assertEqual(reflection["outcome"], "success")
        facts = [(m["key"], m["value"]) for m in reflection["memories_to_create"]
                 if m["type"] == "user_fact"]
        self.assertIn(("user_name", "Alice"), facts)
        # Facts land in the user model AND the (isolated) vector store
        self.assertIn("user_name", engine.user_model.profile["important_facts"])
        self.assertGreaterEqual(len(engine.vector_db), 1)
        self.assertEqual(engine.mode_counts, {"llm": 1, "heuristic": 0})
        self.assertEqual(len(calls), 1)

    def test_cycle_report_carries_reflection_modes_summary(self):
        _, calls = _install_fake_litellm(canned_content=VALID_REFLECTION_JSON)
        self.addCleanup(sys.modules.pop, "litellm", None)

        engine = self.make_engine()

        from learning_loop.scheduler import LearningScheduler
        from learning_loop.pattern_recognizer import PatternRecognizer

        class _StubIndexer:
            last_new_session_messages = {
                "sess_llm_1": list(SESSION_MESSAGES),
            }

            def run_indexing(self, hours_back=24):
                return {
                    "messages_indexed": 3,
                    "sessions_indexed": 1,
                    "newly_indexed_sessions": ["sess_llm_1"],
                }

        recognizer = PatternRecognizer()
        recognizer.vector_db = engine.vector_db

        config = {
            "interval_hours": 2,
            "index_hours_back": 24,
            "pattern_days_back": 7,
            "enable_skill_generation": False,
            "enable_memory_consolidation": False,
            "enable_reflection": True,
            "min_pattern_frequency": 3,
            "daemon_mode": False,
        }
        scheduler = LearningScheduler(
            config=config,
            indexer=_StubIndexer(),
            pattern_recognizer=recognizer,
            reflection_engine=engine,
        )
        # Keep cycle bookkeeping out of repo data/scheduler_state.json.
        scheduler.state_file = os.path.join(self.test_dir, "scheduler_state.json")
        scheduler.state = scheduler._load_state()

        with mock.patch.dict(os.environ, {"OPENAI_API_KEY": "sk-test-canned-key"}):
            from core.llm import OpenMemLLM
            real_llm = OpenMemLLM()
            with mock.patch("core.llm.get_llm", return_value=real_llm):
                report = scheduler.run_cycle()

        self.assertTrue(report["success"], report.get("phase_errors"))
        self.assertEqual(report["reflection_modes"], {"llm": 1, "heuristic": 0})
        self.assertEqual(
            report["phases"]["reflection"]["reflection_modes"],
            {"llm": 1, "heuristic": 0},
        )
        self.assertEqual(report["phases"]["reflection"]["sessions_reflected"], 1)
        stored = engine.reflections["session_reflections"]
        self.assertTrue(all(r.get("mode") == "llm" for r in stored))


# ---------------------------------------------------------------------------
# Regime 2: malformed output -> visible logged fallback, cycle continues
# ---------------------------------------------------------------------------

class TestMalformedLLMReflection(LLMBoundaryTestCase):

    def _available_llm_returning(self, canned_content):
        _install_fake_litellm(canned_content=canned_content)
        self.addCleanup(sys.modules.pop, "litellm", None)
        with mock.patch.dict(os.environ, {"OPENAI_API_KEY": "sk-test-canned-key"}):
            from core.llm import OpenMemLLM
            return OpenMemLLM()

    def test_openmem_llm_reflect_raises_valueerror_on_malformed_json(self):
        llm = self._available_llm_returning(MALFORMED_REFLECTION_TEXT)
        self.assertTrue(llm.is_available)
        with self.assertRaises(ValueError):
            llm.reflect(SESSION_MESSAGES)

    def test_openmem_llm_reflect_raises_on_unrecognized_shape(self):
        llm = self._available_llm_returning(UNRECOGNIZED_SHAPE_JSON)
        with self.assertRaises(ValueError):
            llm.reflect(SESSION_MESSAGES)

    def test_engine_falls_back_to_heuristic_with_logged_warning(self):
        llm = self._available_llm_returning(MALFORMED_REFLECTION_TEXT)
        engine = self.make_engine()

        with self.assertLogs("learning_loop.reflection_engine",
                             level="WARNING") as logs:
            with mock.patch("core.llm.get_llm", return_value=llm):
                reflection = engine.reflect_on_session(SESSION_MESSAGES)

        fallback_logs = [ln for ln in logs.output
                         if "[Reflection] LLM reflection failed" in ln]
        self.assertTrue(fallback_logs,
                        f"expected visible fallback warning, got: {logs.output}")
        self.assertEqual(reflection["mode"], "heuristic")
        self.assertIn("mode_fallback_reason", reflection)
        # Heuristic path still produced a usable outcome + cycle continued
        self.assertIsNotNone(reflection["outcome"])
        self.assertEqual(engine.mode_counts, {"llm": 0, "heuristic": 1})

    def test_scheduler_survives_all_malformed_reflections(self):
        llm = self._available_llm_returning(MALFORMED_REFLECTION_TEXT)
        engine = self.make_engine()

        from learning_loop.scheduler import LearningScheduler
        from learning_loop.pattern_recognizer import PatternRecognizer

        class _StubIndexer:
            last_new_session_messages = {"sess_bad_1": list(SESSION_MESSAGES)}

            def run_indexing(self, hours_back=24):
                return {"messages_indexed": 3, "sessions_indexed": 1,
                        "newly_indexed_sessions": ["sess_bad_1"]}

        recognizer = PatternRecognizer()
        recognizer.vector_db = engine.vector_db

        scheduler = LearningScheduler(
            config={
                "interval_hours": 2, "index_hours_back": 24,
                "pattern_days_back": 7, "enable_skill_generation": False,
                "enable_memory_consolidation": False,
                "enable_reflection": True, "min_pattern_frequency": 3,
                "daemon_mode": False,
            },
            indexer=_StubIndexer(),
            pattern_recognizer=recognizer,
            reflection_engine=engine,
        )
        scheduler.state_file = os.path.join(self.test_dir, "scheduler_state.json")
        scheduler.state = scheduler._load_state()

        with mock.patch("core.llm.get_llm", return_value=llm):
            report = scheduler.run_cycle()

        self.assertTrue(report["success"],
                        "cycle must continue despite malformed LLM output")
        self.assertEqual(report["reflection_modes"], {"llm": 0, "heuristic": 1})


# ---------------------------------------------------------------------------
# Regime 3: no-key environment -> zero network attempts
# ---------------------------------------------------------------------------

class TestNoKeyEnvironment(LLMBoundaryTestCase):
    """setUp clears all hosted-provider key vars."""

    def test_init_performs_zero_network_attempts_without_keys(self):
        _, calls = _install_fake_litellm(canned_content=VALID_REFLECTION_JSON)
        self.addCleanup(sys.modules.pop, "litellm", None)

        from core.llm import OpenMemLLM
        llm = OpenMemLLM()

        self.assertFalse(llm.is_available)
        self.assertEqual(calls, [])
        self.assertEqual(llm.provider, "heuristic")

    def test_reflect_runs_heuristic_without_any_completion_call(self):
        _, calls = _install_fake_litellm(canned_content=VALID_REFLECTION_JSON)
        self.addCleanup(sys.modules.pop, "litellm", None)

        from core.llm import OpenMemLLM
        llm = OpenMemLLM()
        result = llm.reflect(SESSION_MESSAGES)

        self.assertEqual(result["outcome"], "success")  # heuristic detection
        self.assertEqual(calls, [], "heuristic mode must never hit the wire")

    def test_engine_reports_heuristic_mode_in_no_key_environment(self):
        _, calls = _install_fake_litellm(canned_content=VALID_REFLECTION_JSON)
        self.addCleanup(sys.modules.pop, "litellm", None)
        engine = self.make_engine()

        reflection = engine.reflect_on_session(SESSION_MESSAGES)

        self.assertEqual(reflection["mode"], "heuristic")
        self.assertEqual(engine.mode_counts, {"llm": 0, "heuristic": 1})
        self.assertEqual(calls, [])

    def test_missing_litellm_package_degrades_to_heuristic(self):
        # Simulate the optional dependency being absent entirely.
        saved = sys.modules.pop("litellm", None)
        self.addCleanup(lambda: sys.modules.__setitem__("litellm", saved)
                        if saved else None)

        from core.llm import OpenMemLLM
        llm = OpenMemLLM()
        self.assertFalse(llm.is_available)
        self.assertIsNone(llm._litellm)


if __name__ == "__main__":
    unittest.main()
