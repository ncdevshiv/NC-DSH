#!/usr/bin/env python3
"""
OpenMem Memory Skill for Antigravity IDE.

Usage:
    /mem          - Show help
    /mem status   - System status
    /mem search <query> - Search memories
    /mem run      - Run learning cycle

This skill auto-activates when the user mentions memory, recall, or past conversations.
"""

import os
import sys
import json
from pathlib import Path
from datetime import datetime

# Add OpenMem to path
OPENMEM_ROOT = Path(__file__).parent.parent.parent.parent
sys.path.insert(0, str(OPENMEM_ROOT))


def get_context():
    """Get execution context."""
    return {
        "agent": "Antigravity IDE",
        "agent_key": "antigravity_ide",
        "skill_version": "1.0.0",
        "timestamp": datetime.now().isoformat(),
    }


def cmd_status(args, context):
    """Show OpenMem status."""
    try:
        from memory_store.vector_db import get_vector_db
        from learning_loop.scheduler import LearningScheduler

        db = get_vector_db()
        stats = db.get_stats()
        scheduler = LearningScheduler()
        sched = scheduler.get_status()

        return {
            "response": f"""📊 **OpenMem Status (Antigravity IDE)**

**Database:**
- Tables: {', '.join(stats.get('tables', ['none']))}
- Memories: {stats.get('table_memories_rows', 0)}

**Scheduler:**
- Last cycle: {sched.get('last_cycle', 'Never')}
- Cycles: {sched.get('cycles_completed', 0)}""",
            "success": True,
        }
    except Exception as e:
        return {"response": f"❌ Error: {e}", "success": False}


def cmd_search(args, context):
    """Search memories."""
    if not args:
        return {"response": "Usage: /mem search <query>", "success": False}

    try:
        from memory_store.vector_db import get_vector_db
        db = get_vector_db()
        query = " ".join(args)
        results = db.search(query, n_results=5)

        if not results:
            return {"response": f"🔍 No memories found for: **{query}**", "success": True}

        lines = [f"🔍 **Search: \"{query}\"**\n"]
        for i, r in enumerate(results, 1):
            content = r.get("content", "")[:200]
            lines.append(f"{i}. {content}")
            if r.get("metadata", {}).get("tier"):
                lines.append(f"   [{r['metadata']['tier']}] importance: {r.get('importance', 0):.2f}")

        return {"response": "\n".join(lines), "success": True}
    except Exception as e:
        return {"response": f"❌ Error: {e}", "success": False}


def cmd_run(args, context):
    """Run learning cycle."""
    try:
        from learning_loop.scheduler import LearningScheduler
        scheduler = LearningScheduler()
        report = scheduler.run_cycle()

        phases = report.get("phases", {})
        indexing = phases.get("indexing", {})
        patterns = phases.get("pattern_recognition", {})
        skills = phases.get("skill_generation", {})

        return {
            "response": f"""🧠 **Learning Cycle Complete**

⏱️ Duration: {report.get('duration_seconds', 0):.1f}s
📊 Messages indexed: {indexing.get('messages_indexed', 0)}
🎯 Patterns found: {patterns.get('patterns_found', 0)}
🛠️ Skills generated: {skills.get('skills_generated', 0)}""",
            "success": report.get("success", False),
        }
    except Exception as e:
        return {"response": f"❌ Error: {e}", "success": False}


def cmd_patterns(args, context):
    """Show patterns."""
    try:
        from learning_loop.pattern_recognizer import PatternRecognizer
        pr = PatternRecognizer()
        patterns = pr.find_recurring_patterns(days_back=7)

        if not patterns:
            return {"response": "🔍 No patterns discovered yet.", "success": True}

        lines = ["🔍 **Discovered Patterns**\n"]
        for i, p in enumerate(patterns[:10], 1):
            lines.append(f"{i}. **{p['type']}**: {p['pattern']} (freq: {p['frequency']}, conf: {p.get('confidence', 0):.0%})")

        return {"response": "\n".join(lines), "success": True}
    except Exception as e:
        return {"response": f"❌ Error: {e}", "success": False}


def cmd_skills(args, context):
    """List skills."""
    try:
        from memory_store.skill_generator import SkillGenerator
        gen = SkillGenerator()
        skills = gen.get_generated_skills()

        if not skills:
            return {"response": "🛠️ No auto-generated skills yet.", "success": True}

        lines = ["🛠️ **Auto-Generated Skills**\n"]
        for s in skills:
            lines.append(f"- **{s.get('name', 'unknown')}**: triggers: {s.get('triggers', [])[:3]}")

        return {"response": "\n".join(lines), "success": True}
    except Exception as e:
        return {"response": f"❌ Error: {e}", "success": False}


def cmd_stats(args, context):
    """Show statistics."""
    try:
        from memory_store.vector_db import get_vector_db
        from learning_loop.scheduler import LearningScheduler
        from autonomous import get_optimizer, EvolutionEngine

        db = get_vector_db()
        stats = db.get_stats()
        scheduler = LearningScheduler()
        sched = scheduler.get_status()
        optimizer = get_optimizer()
        opt = optimizer.get_stats()
        evolution = EvolutionEngine()
        evo = evolution.get_stats()

        return {
            "response": f"""📈 **OpenMem Statistics**

**Database:**
- Tables: {len(stats.get('tables', []))}

**Learning:**
- Cycles: {sched.get('cycles_completed', 0)}
- Messages indexed: {sched.get('stats', {}).get('total_messages_indexed', 0)}

**Optimization:**
- Entities: {opt.get('total_entities', 0)}

**Evolution:**
- Generation: {evo.get('generation', 0)}
- Best fitness: {evo.get('best_fitness', 0):.3f}""",
            "success": True,
        }
    except Exception as e:
        return {"response": f"❌ Error: {e}", "success": False}


def cmd_profile(args, context):
    """Show user profile."""
    try:
        from memory_store.vector_db import get_vector_db
        db = get_vector_db()
        profiles = db.get_all_user_profiles()

        if not profiles:
            return {"response": "👤 No profile data yet.", "success": True}

        lines = ["👤 **User Profile**\n"]
        for key, data in profiles.items():
            lines.append(f"- **{key}**: {data['value']} ({data['confidence']:.0%})")

        return {"response": "\n".join(lines), "success": True}
    except Exception as e:
        return {"response": f"❌ Error: {e}", "success": False}


def cmd_optimize(args, context):
    """Run optimization."""
    try:
        from autonomous import get_optimizer
        optimizer = get_optimizer()
        report = optimizer.run_optimization_cycle()
        return {
            "response": f"🔄 **Optimization Complete**\n\nPruned: {len(report.get('pruned', []))}\nStrengthened: {len(report.get('strengthened', []))}",
            "success": True,
        }
    except Exception as e:
        return {"response": f"❌ Error: {e}", "success": False}


def cmd_evolve(args, context):
    """Run evolution."""
    try:
        from autonomous import EvolutionEngine
        engine = EvolutionEngine()
        report = engine.evolve()
        return {
            "response": f"🧬 **Evolution Complete**\n\nGeneration: {report.get('generation', 0)}\nBest fitness: {report.get('best_fitness', 0):.3f}",
            "success": True,
        }
    except Exception as e:
        return {"response": f"❌ Error: {e}", "success": False}


COMMANDS = {
    "status": cmd_status,
    "search": cmd_search,
    "run": cmd_run,
    "patterns": cmd_patterns,
    "skills": cmd_skills,
    "stats": cmd_stats,
    "profile": cmd_profile,
    "optimize": cmd_optimize,
    "evolve": cmd_evolve,
    "help": lambda a, c: {
        "response": f"""📖 **OpenMem Help (Antigravity IDE)**

Commands:
  status     - System status
  search <q> - Search memories
  run        - Run learning cycle
  patterns   - Show patterns
  skills     - List skills
  stats      - Statistics
  profile    - User profile
  optimize   - Run optimization
  evolve     - Run evolution""",
        "success": True,
    },
}


def execute(command, args=None, context=None):
    """Main entry point."""
    args = args or []
    context = context or {}
    context.update(get_context())

    if command not in COMMANDS:
        return COMMANDS["help"](args, context)

    try:
        return COMMANDS[command](args, context)
    except Exception as e:
        import traceback
        return {
            "response": f"❌ Error: {e}\n\n_{traceback.format_exc()}_",
            "success": False,
        }


if __name__ == "__main__":
    if len(sys.argv) < 2:
        result = COMMANDS["help"]([], {})
    else:
        result = execute(sys.argv[1], sys.argv[2:])
    print(result["response"])
    sys.exit(0 if result.get("success") else 1)
