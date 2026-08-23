"""
Learning Loop Scheduler for OpenMem.
Manages automated scheduling of learning cycles.
"""

import os
import json
import time
import logging
import threading
import traceback
from datetime import datetime, timedelta
from typing import Dict, List, Any, Optional, Callable
from pathlib import Path

from .conversation_indexer import ConversationIndexer
from .pattern_recognizer import PatternRecognizer
from .reflection_engine import ReflectionEngine
from memory_store.skill_generator import SkillGenerator

logger = logging.getLogger(__name__)


class LearningScheduler:
    """
    Schedules and orchestrates the autonomous learning loop.

    Learning cycle phases (5, run in order):
    1. Index recent conversations
    2. Recognize patterns
    3. Generate/update skills (config-gated)
    4. Run reflection engine (config-gated)
    5. Consolidate memory (config-gated)

    Phases run independently: a phase failure is logged with its name and
    recorded in report["phase_errors"] without aborting later phases; the
    cycle continues and report["success"] is set False if any phase failed.

    Can run:
    - On a schedule (e.g., every 2 hours)
    - On demand (triggered manually)
    - As a daemon process
    """
    
    def __init__(self, config: Dict = None,
                 indexer: Any = None, pattern_recognizer: Any = None,
                 reflection_engine: Any = None, skill_generator: Any = None):
        self.config = config or self._default_config()

        # Initialize components. Each accepts an injected instance (tests /
        # programmatic use — mirrors ConversationIndexer's vector_db DI);
        # production always builds the shared default.
        self.indexer = indexer if indexer is not None else ConversationIndexer()
        self.pattern_recognizer = (
            pattern_recognizer if pattern_recognizer is not None
            else PatternRecognizer()
        )
        self.reflection_engine = (
            reflection_engine if reflection_engine is not None
            else ReflectionEngine()
        )
        self.skill_generator = (
            skill_generator if skill_generator is not None
            else SkillGenerator()
        )
        
        # State
        self.state_file = os.path.join(
            os.path.dirname(__file__), "..", "data", "scheduler_state.json"
        )
        os.makedirs(os.path.dirname(os.path.abspath(self.state_file)), exist_ok=True)
        self.state = self._load_state()
        
        # Threading for daemon mode
        self._stop_event = threading.Event()
        self._daemon_thread = None
        
        # Callbacks
        self.on_cycle_complete = None  # Optional callback when cycle completes
        self.on_improvement_found = None
    
    def _default_config(self) -> Dict:
        """Default configuration."""
        return {
            "interval_hours": 2,           # Hours between automatic cycles
            "index_hours_back": 24,        # How far back to index conversations
            "pattern_days_back": 7,        # Days to analyze for patterns
            "enable_skill_generation": True,
            "enable_memory_consolidation": True,
            "enable_reflection": True,
            "min_pattern_frequency": 3,    # Min occurrences to generate skill
            "daemon_mode": False
        }
    
    def _load_state(self) -> Dict:
        """Load scheduler state."""
        if os.path.exists(self.state_file):
            with open(self.state_file, 'r') as f:
                return json.load(f)
        return {
            "last_cycle": None,
            "cycles_completed": 0,
            "cycles_failed": 0,
            "last_cycle_duration": None,
            "total_messages_indexed": 0,
            "skills_generated": 0,
            "improvements_made": 0
        }
    
    def _save_state(self):
        """Save scheduler state."""
        with open(self.state_file, 'w') as f:
            json.dump(self.state, f, indent=2)
    
    def _run_phase(self, report: Dict, phase_name: str, phase_fn: Callable[[], Any]) -> Optional[Any]:
        """
        Run a single learning-cycle phase, isolating its failures.

        On success the phase result is stored under report["phases"][phase_name].
        On failure the error is logged with the phase name, recorded in
        report["phase_errors"][phase_name], report["success"] is set False, and
        None is returned so the rest of the cycle can continue.

        Args:
            report: Cycle report dict to update
            phase_name: Identifier of the phase (e.g. "indexing")
            phase_fn: Zero-argument callable performing the phase work

        Returns:
            Phase result on success, otherwise None
        """
        try:
            result = phase_fn()
            report["phases"][phase_name] = result
            return result
        except Exception as e:
            logger.exception(f"[Scheduler] Phase '{phase_name}' failed: {e}")
            report.setdefault("phase_errors", {})[phase_name] = str(e)
            report["success"] = False
            return None

    def run_cycle(self, full: bool = False) -> Dict:
        """
        Run a complete learning cycle.

        Args:
            full: If True, do a full re-index (not just recent)

        Returns:
            Cycle report dict; failed phases are listed in report["phase_errors"]
        """
        start_time = datetime.now()
        report = {
            "started_at": start_time.isoformat(),
            "phases": {},
            "phase_errors": {},
            "success": True,
            "errors": []
        }

        logger.info(f"[Scheduler] Starting learning cycle at {start_time.isoformat()}")

        try:
            # Phase 1: Index recent conversations
            def _index_phase() -> Dict:
                logger.info("[Scheduler] Phase 1: Indexing conversations...")
                index_report = self.indexer.run_indexing(
                    hours_back=self.config["index_hours_back"] if not full else 24 * 30
                )
                self.state["total_messages_indexed"] += index_report.get("messages_indexed", 0)
                logger.info(
                    f"[Scheduler] Indexed {index_report.get('messages_indexed', 0)} messages "
                    f"from {index_report.get('sessions_indexed', 0)} sessions"
                )
                return index_report

            self._run_phase(report, "indexing", _index_phase)

            # Phase 2: Pattern recognition
            def _pattern_phase() -> Dict:
                logger.info("[Scheduler] Phase 2: Recognizing patterns...")
                patterns = self.pattern_recognizer.find_recurring_patterns(
                    days_back=self.config["pattern_days_back"]
                )
                logger.info(f"[Scheduler] Found {len(patterns)} patterns")
                return {
                    "patterns_found": len(patterns),
                    "patterns": patterns[:5]  # Top 5
                }

            self._run_phase(report, "pattern_recognition", _pattern_phase)

            # Phase 3: Skill generation
            if self.config["enable_skill_generation"]:
                def _skill_phase() -> Dict:
                    logger.info("[Scheduler] Phase 3: Generating skills...")
                    new_skills = self.skill_generator.generate_all_skills_from_patterns(
                        min_frequency=self.config["min_pattern_frequency"]
                    )
                    self.state["skills_generated"] += len(new_skills)
                    logger.info(f"[Scheduler] Generated {len(new_skills)} skills")
                    return {
                        "skills_generated": len(new_skills),
                        "skills": [s.get("name") for s in new_skills if s]
                    }

                self._run_phase(report, "skill_generation", _skill_phase)
            else:
                report["phases"]["skill_generation"] = {"skipped": True}

            # Phase 4: Reflection
            if self.config["enable_reflection"]:
                def _reflection_phase() -> Dict:
                    logger.info("[Scheduler] Phase 4: Running reflection...")
                    reflection_report = self.reflection_engine.run_self_check()

                    # Reflect on sessions freshly indexed THIS cycle so
                    # reflections reference real conversation content (the
                    # heuristic path runs without any LLM backend).
                    new_session_messages = getattr(
                        self.indexer, "last_new_session_messages", {}
                    )
                    sessions_reflected = 0
                    for sid, msgs in sorted(new_session_messages.items()):
                        try:
                            self.reflection_engine.reflect_on_session(msgs)
                            sessions_reflected += 1
                        except Exception as e:
                            logger.warning(
                                f"[Scheduler] Reflection failed for session "
                                f"{sid}: {e}"
                            )
                    if sessions_reflected:
                        logger.info(
                            f"[Scheduler] Reflected on {sessions_reflected} "
                            f"newly indexed session(s)"
                        )
                    reflection_report["sessions_reflected"] = sessions_reflected
                    reflection_report["reflection_modes"] = dict(
                        self.reflection_engine.mode_counts
                    )

                    improvements = reflection_report.get("improvements_completed", 0)
                    self.state["improvements_made"] += improvements
                    logger.info(f"[Scheduler] Reflection complete, {improvements} improvements made")
                    return reflection_report

                self._run_phase(report, "reflection", _reflection_phase)
                # Cycle-level summary: how many reflections ran per mode.
                report["reflection_modes"] = dict(
                    self.reflection_engine.mode_counts
                )
            else:
                report["phases"]["reflection"] = {"skipped": True}

            # Phase 5: Memory consolidation
            if self.config["enable_memory_consolidation"]:
                def _consolidation_phase() -> Dict:
                    logger.info("[Scheduler] Phase 5: Consolidating memory...")
                    consolidation = self.reflection_engine.memory_manager.run_consolidation()
                    logger.info(
                        f"[Scheduler] Consolidation: "
                        f"{consolidation.get('weekly_created', 0)} weekly summaries created"
                    )
                    return consolidation

                self._run_phase(report, "memory_consolidation", _consolidation_phase)
            else:
                report["phases"]["memory_consolidation"] = {"skipped": True}

            # Update state
            self.state["last_cycle"] = datetime.now().isoformat()
            self.state["cycles_completed"] += 1

        except Exception as e:
            report["success"] = False
            report["error"] = str(e)
            report["traceback"] = traceback.format_exc()
            self.state["cycles_failed"] += 1
            logger.exception(f"[Scheduler] Cycle failed: {e}")

        # Calculate duration
        end_time = datetime.now()
        duration = (end_time - start_time).total_seconds()
        report["completed_at"] = end_time.isoformat()
        report["duration_seconds"] = duration
        self.state["last_cycle_duration"] = duration

        self._save_state()

        # Trigger callback if set
        if self.on_cycle_complete:
            try:
                self.on_cycle_complete(report)
            except Exception as e:
                logger.error(f"[Scheduler] Cycle-complete callback raised: {e}")

        logger.info(f"[Scheduler] Cycle completed in {duration:.1f}s")
        return report
    
    def run_cycle_async(self, full: bool = False) -> threading.Thread:
        """Run a learning cycle in a background thread."""
        def run():
            self.run_cycle(full=full)
        
        thread = threading.Thread(target=run, daemon=True)
        thread.start()
        return thread
    
    def start_daemon(self, interval_hours: float = None):
        """
        Start the scheduler as a daemon process.
        
        Args:
            interval_hours: Override config interval
        """
        if interval_hours:
            self.config["interval_hours"] = interval_hours
        
        self._stop_event.clear()
        
        def daemon_loop():
            while not self._stop_event.is_set():
                # Run a cycle
                self.run_cycle()
                
                # Wait for next interval or stop signal
                interval_ms = self.config["interval_hours"] * 60 * 60 * 1000
                self._stop_event.wait(timeout=interval_ms / 1000)
        
        self._daemon_thread = threading.Thread(target=daemon_loop, daemon=True)
        self._daemon_thread.start()
        print(f"[Scheduler] Daemon started, running cycles every {self.config['interval_hours']} hours")
        
        return self._daemon_thread
    
    def stop_daemon(self):
        """Stop the daemon process."""
        if self._daemon_thread:
            self._stop_event.set()
            self._daemon_thread.join(timeout=5)
            self._daemon_thread = None
            print("[Scheduler] Daemon stopped")
    
    def get_next_scheduled_run(self) -> Optional[datetime]:
        """Get when the next scheduled cycle will run."""
        if not self.state.get("last_cycle"):
            return datetime.now()
        
        last = datetime.fromisoformat(self.state["last_cycle"])
        interval = timedelta(hours=self.config["interval_hours"])
        return last + interval
    
    def get_status(self) -> Dict:
        """Get current scheduler status."""
        next_run = self.get_next_scheduled_run()
        next_run_str = next_run.isoformat() if next_run else None
        
        time_to_next = None
        if next_run:
            delta = (next_run - datetime.now()).total_seconds()
            if delta > 0:
                time_to_next = delta
        
        return {
            "daemon_running": self._daemon_thread is not None and self._daemon_thread.is_alive(),
            "last_cycle": self.state.get("last_cycle"),
            "next_scheduled_run": next_run_str,
            "time_to_next_run_seconds": time_to_next,
            "cycles_completed": self.state.get("cycles_completed", 0),
            "cycles_failed": self.state.get("cycles_failed", 0),
            "last_duration_seconds": self.state.get("last_cycle_duration"),
            "stats": {
                "total_messages_indexed": self.state.get("total_messages_indexed", 0),
                "skills_generated": self.state.get("skills_generated", 0),
                "improvements_made": self.state.get("improvements_made", 0)
            }
        }
    
    def trigger_now(self) -> Dict:
        """Trigger an immediate cycle."""
        return self.run_cycle()
    
    def reset_state(self):
        """Reset all scheduler state."""
        self.state = {
            "last_cycle": None,
            "cycles_completed": 0,
            "cycles_failed": 0,
            "last_cycle_duration": None,
            "total_messages_indexed": 0,
            "skills_generated": 0,
            "improvements_made": 0
        }
        self._save_state()
        print("[Scheduler] State reset")


def create_cron_integration() -> str:
    """
    Generate cron job commands for OpenClaw integration.
    Returns string with cron setup commands.
    """
    base_dir = os.path.dirname(__file__)
    main_py = os.path.join(os.path.dirname(base_dir), "main.py")
    
    commands = f"""
# OpenMem Learning Loop - Cron Setup

# Run learning cycle every 2 hours
0 */2 * * * python "{main_py}" run-cycle >> /var/log/openmem/cycles.log 2>&1

# Run full re-index daily at 3 AM
0 3 * * * python "{main_py}" run-cycle --full >> /var/log/openmem/full_index.log 2>&1

# Check status daily
0 9 * * * python "{main_py}" status
""".strip()
    
    return commands
