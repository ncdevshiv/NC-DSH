#!/usr/bin/env python3
"""
LanceMem Agent Wrapper
Wraps ANY AI agent with autonomous memory capabilities.

Usage:
    from skills.generic.lance_mem_agent import LanceMemAgent
    
    agent = LanceMemAgent(your_existing_agent)
    agent.run("task")  # With memory!
"""

import os
import sys
import logging
from pathlib import Path
from typing import Any, Optional, List, Dict
from datetime import datetime
import threading

logger = logging.getLogger("openmem.lance_mem_agent")

# LanceMem root
LANCE_MEM_ROOT = Path(__file__).parent.parent.parent
sys.path.insert(0, str(LANCE_MEM_ROOT))


class LanceMemAgent:
    """
    Wraps any AI agent with LanceMem memory.
    
    Features:
    - Automatic context injection from memory
    - Message logging to vector database
    - Scheduled learning cycles
    - Pattern recognition
    - Skill auto-generation
    - Self-optimization
    - Self-evolution
    
    Args:
        agent: Any existing agent object (LangChain, AutoGPT, CrewAI, etc.)
        auto_memory: If True, automatically search and inject memory
        auto_learn: If True, run learning cycles automatically
        learn_interval: Seconds between learning cycles (default: 7200 = 2 hours)
        db_path: Custom LanceDB path
        min_importance: Minimum importance to store (0.0-1.0)
    """

    def __init__(
        self,
        agent: Any,
        auto_memory: bool = True,
        auto_learn: bool = True,
        learn_interval: int = 7200,
        db_path: Optional[str] = None,
        min_importance: float = 0.5
    ):
        self.agent = agent
        self.auto_memory = auto_memory
        self.auto_learn = auto_learn
        self.learn_interval = learn_interval
        self.min_importance = min_importance
        
        # Initialize LanceMem components
        self._init_memory(db_path)
        
        # Start background learning if enabled
        self._learn_thread = None
        self._stop_learn = threading.Event()
        
        if self.auto_learn:
            self._start_auto_learn()

    def _init_memory(self, db_path: Optional[str] = None):
        """Initialize memory components."""
        try:
            from memory_store.vector_db import get_vector_db, LanceDBVectorStore
            
            if db_path:
                self.db = LanceDBVectorStore(db_path=db_path)
            else:
                self.db = get_vector_db()
            
            self.db_path = self.db.db_path
            
        except ImportError as e:
            print(f"[LanceMemAgent] Memory init failed: {e}")
            self.db = None

        try:
            from memory_store.user_model import UserModel
            self.user_model = UserModel()
        except Exception as e:
            logger.debug(f"User model init failed: {e}")
            self.user_model = None

        try:
            from memory_store.memory_manager import MemoryManager
            self.memory_manager = MemoryManager()
        except Exception as e:
            logger.debug(f"Memory manager init failed: {e}")
            self.memory_manager = None

        try:
            from autonomous import get_optimizer, EvolutionEngine
            self.optimizer = get_optimizer()
            self.evolution = EvolutionEngine()
        except Exception as e:
            logger.debug(f"Optimizer/evolution init failed: {e}")
            self.optimizer = None
            self.evolution = None

    def _start_auto_learn(self):
        """Start background learning thread."""
        def learn_loop():
            import time
            while not self._stop_learn.wait(self.learn_interval):
                try:
                    self.run_cycle()
                except Exception as e:
                    print(f"[LanceMemAgent] Auto-learn error: {e}")
        
        self._learn_thread = threading.Thread(target=learn_loop, daemon=True)
        self._learn_thread.start()

    def stop(self):
        """Stop auto-learning and cleanup."""
        self._stop_learn.set()
        if self._learn_thread:
            self._learn_thread.join(timeout=5)

    # ===== Core Agent Methods =====

    def run(self, task: str, **kwargs) -> str:
        """
        Run the agent with memory context.
        
        Args:
            task: User task/command
            **kwargs: Additional args passed to underlying agent
            
        Returns:
            Agent response string
        """
        # Inject memory context if enabled
        context = ""
        if self.auto_memory and self.db:
            memories = self.search(task)
            if memories:
                context = self._format_context(memories)
        
        # Build prompt with context
        if context:
            task = f"{context}\n\nTask: {task}"
        
        # Execute agent
        if hasattr(self.agent, 'run'):
            response = self.agent.run(task, **kwargs)
        elif hasattr(self.agent, '__call__'):
            response = self.agent(task, **kwargs)
        else:
            response = f"Error: Agent has no run() or __call__ method"
        
        # Record interaction
        self.add_message("user", task)
        self.add_message("agent", response)
        
        return response

    def think(self, task: str) -> str:
        """Alias for run()."""
        return self.run(task)

    def __call__(self, task: str) -> str:
        """Allow agent to be called directly."""
        return self.run(task)

    # ===== Memory Operations =====

    def search(self, query: str, n_results: int = 5) -> List[Dict]:
        """
        Search memory for relevant context.
        
        Args:
            query: Search query
            n_results: Number of results to return
            
        Returns:
            List of memory dicts with 'content' and metadata
        """
        if not self.db:
            return []
        
        try:
            return self.db.search(query, n_results=n_results)
        except Exception as e:
            print(f"[LanceMemAgent] Search error: {e}")
            return []

    def add_message(
        self,
        role: str,
        content: str,
        importance: Optional[float] = None,
        metadata: Optional[Dict] = None
    ) -> bool:
        """
        Add a message to memory.
        
        Args:
            role: "user" or "agent"
            content: Message content
            importance: 0.0-1.0 importance score (auto-calculated if None)
            metadata: Additional metadata
            
        Returns:
            True if successful
        """
        if not self.db:
            return False
        
        # Auto-calculate importance
        if importance is None:
            importance = self._calculate_importance(content, role)
        
        try:
            self.db.add_memory(
                content=content,
                session_id=getattr(self.agent, 'session_id', None),
                importance=importance,
                tags=[role],
                metadata=metadata or {"role": role}
            )
            
            # Update user model if user message
            if role == "user" and self.user_model:
                try:
                    self.user_model.analyze_message(content)
                except Exception as e:
                    logger.debug(f"User model analysis failed: {e}")
            
            return True
        except Exception as e:
            print(f"[LanceMemAgent] Add message error: {e}")
            return False

    def _calculate_importance(self, content: str, role: str) -> float:
        """Auto-calculate message importance."""
        importance = 0.5
        
        # User messages are slightly more important
        if role == "user":
            importance += 0.1
        
        # Important keywords boost importance
        important_keywords = [
            "remember", "important", "don't forget", "my name",
            "preference", "always", "never", "project", "deadline"
        ]
        if any(kw in content.lower() for kw in important_keywords):
            importance += 0.2
        
        # Long messages might be context-rich
        if len(content) > 500:
            importance += 0.1
        
        return min(1.0, importance)

    def _format_context(self, memories: List[Dict]) -> str:
        """Format memories for prompt injection."""
        if not memories:
            return ""
        
        lines = ["[Relevant memories from past conversations]:\n"]
        for i, mem in enumerate(memories, 1):
            content = mem.get('content', '')[:200]
            role = mem.get('metadata', {}).get('role', 'unknown')
            lines.append(f"{i}. [{role}] {content}...")
        
        return "\n".join(lines)

    # ===== Learning Operations =====

    def run_cycle(self, full: bool = False) -> Dict:
        """
        Run a full learning cycle.
        
        Args:
            full: If True, do full re-index
            
        Returns:
            Cycle report dict
        """
        try:
            from learning_loop.scheduler import LearningScheduler
            
            scheduler = LearningScheduler()
            return scheduler.run_cycle(full=full)
        except Exception as e:
            print(f"[LanceMemAgent] Learning cycle error: {e}")
            return {"error": str(e)}

    def optimize(self) -> Dict:
        """
        Run optimization cycle.
        
        Returns:
            Optimization report dict
        """
        if not self.optimizer:
            return {"error": "Optimizer not available"}
        
        try:
            return self.optimizer.run_optimization_cycle()
        except Exception as e:
            print(f"[LanceMemAgent] Optimize error: {e}")
            return {"error": str(e)}

    def evolve(self) -> Dict:
        """
        Run evolution cycle.
        
        Returns:
            Evolution report dict
        """
        if not self.evolution:
            return {"error": "Evolution not available"}
        
        try:
            return self.evolution.evolve()
        except Exception as e:
            print(f"[LanceMemAgent] Evolve error: {e}")
            return {"error": str(e)}

    # ===== Profile & Status =====

    def get_profile(self) -> Dict:
        """Get user profile summary."""
        if not self.user_model:
            return {"error": "User model not available"}
        
        try:
            return {
                "summary": self.user_model.get_profile_summary(),
                "style": self.user_model.get_preferred_response_style(),
                "facts": self.user_model.profile.get("important_facts", {}),
                "topics": self.user_model.get_preferred_topics(),
                "active_hours": self.user_model.get_active_hours()
            }
        except Exception as e:
            return {"error": str(e)}

    def status(self) -> Dict:
        """Get system status."""
        status = {
            "agent_type": type(self.agent).__name__,
            "auto_memory": self.auto_memory,
            "auto_learn": self.auto_learn,
            "learn_interval": self.learn_interval
        }
        
        if self.db:
            status["memory"] = {
                "type": "LanceDB",
                "path": self.db.db_path,
                "count": len(self.db) if hasattr(self.db, '__len__') else 'unknown'
            }
        
        if self.optimizer:
            try:
                opt_stats = self.optimizer.get_stats()
                status["optimizer"] = {
                    "entities": opt_stats.get("total_entities", 0),
                    "matrix_size": opt_stats.get("matrix_size", 0)
                }
            except Exception as e:
                logger.debug(f"Optimizer stats failed: {e}")

        if self.evolution:
            try:
                evo_stats = self.evolution.get_stats()
                status["evolution"] = {
                    "generation": evo_stats.get("generation", 0),
                    "population": evo_stats.get("population_size", 0),
                    "best_fitness": evo_stats.get("best_fitness", 0)
                }
            except Exception as e:
                logger.debug(f"Evolution stats failed: {e}")
        
        return status

    # ===== Context Injection =====

    def get_context_for_prompt(self, query: str) -> str:
        """
        Get formatted memory context for manual prompt injection.
        
        Args:
            query: Current query/task
            
        Returns:
            Formatted context string for injection
        """
        memories = self.search(query)
        return self._format_context(memories)

    def inject_context(self, prompt: str, query: str) -> str:
        """
        Inject memory context into a prompt.
        
        Args:
            prompt: Original prompt
            query: Current task/query
            
        Returns:
            Prompt with context injected
        """
        context = self.get_context_for_prompt(query)
        if context:
            return f"{context}\n\n{prompt}"
        return prompt

    # ===== Cleanup =====

    def __del__(self):
        """Cleanup on delete."""
        try:
            self.stop()
        except Exception as e:
            logger.debug(f"Cleanup failed: {e}")

    def __enter__(self):
        """Context manager entry."""
        return self

    def __exit__(self, exc_type, exc_val, exc_tb):
        """Context manager exit."""
        self.stop()


# Factory function for easy creation
def create_with_memory(agent: Any, **kwargs) -> LanceMemAgent:
    """
    Create a LanceMem-enabled version of any agent.
    
    Args:
        agent: Your existing agent
        **kwargs: Additional args for LanceMemAgent
        
    Returns:
        LanceMemAgent wrapper
    """
    return LanceMemAgent(agent, **kwargs)


# Example usage (commented out)
if __name__ == "__main__":
    # Example: Wrap a simple agent
    class DummyAgent:
        def run(self, task):
            return f"Response to: {task}"
    
    agent = DummyAgent()
    memory_agent = LanceMemAgent(agent)
    
    # Use normally
    print(memory_agent.run("Hello"))
    print(memory_agent.run("Build me a website"))
    
    # Check status
    import json
    print(json.dumps(memory_agent.status(), indent=2))
