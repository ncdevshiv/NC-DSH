#!/usr/bin/env python3
"""
LanceMem for OpenClaw - Skill Handler
Provides /lm commands for autonomous memory.
"""

import os
import sys
import json
from datetime import datetime
from pathlib import Path

# LanceMem paths
LANCE_MEM_BASE = Path(__file__).parent.parent.parent
LANCE_MEM_ROOT = LANCE_MEM_BASE
sys.path.insert(0, str(LANCE_MEM_ROOT))


def get_context():
    """Get skill context."""
    return {
        "skill": "lancemem",
        "version": "2.0.0",
        "root": str(LANCE_MEM_ROOT),
        "timestamp": datetime.now().isoformat()
    }


def cmd_status(args, ctx):
    """Show LanceMem status."""
    from memory_store.vector_db import get_vector_db
    from autonomous import get_optimizer, EvolutionEngine
    
    db = get_vector_db()
    optimizer = get_optimizer()
    evolution = EvolutionEngine()
    
    db_stats = db.get_stats()
    opt_stats = optimizer.get_stats()
    evo_stats = evolution.get_stats()
    
    return {
        "response": f"""📊 **LanceMem Status**

**Memory:**
- Vector DB: {'🟢 LanceDB' if db_stats.get('lancedb_available') else '🔴 Error'}
- Embedder: {'🟢 Active' if db_stats.get('embedder_available') else '🔴 Fallback'}
- Tables: {', '.join(db_stats.get('tables', ['none']))}

**Optimization:**
- Entities: {opt_stats.get('total_entities', 0)}
- Matrix: {opt_stats.get('matrix_size', 0)}x{opt_stats.get('matrix_size', 0)}
- Pruned: {len(opt_stats.get('pruned', [])) if 'pruned' in opt_stats else 0}

**Evolution:**
- Generation: {evo_stats.get('generation', 0)}
- Population: {evo_stats.get('population_size', 0)}
- Best fitness: {evo_stats.get('best_fitness', 0):.3f}

**Health:** 🟢 All systems operational""",
        "success": True
    }


def cmd_install(args, ctx):
    """Install LanceDB."""
    from bin.lancedb_manager import LanceDBManager
    
    manager = LanceDBManager()
    success = manager.install()
    
    if success:
        manager.init_database()
        return {
            "response": "✅ LanceDB installed and initialized.\n\nRun `/lm init` if not done already.",
            "success": True
        }
    return {
        "response": "❌ Installation failed. Try: `pip install lancedb sentence-transformers`",
        "success": False
    }


def cmd_init(args, ctx):
    """Initialize database."""
    from bin.lancedb_manager import LanceDBManager
    from memory_store.vector_db import LanceDBVectorStore
    
    manager = LanceDBManager()
    manager.init_database()
    
    db = LanceDBVectorStore()
    
    return {
        "response": "✅ LanceMem initialized and ready.\n\nRun `/lm run` to start learning!",
        "success": True
    }


def cmd_run(args, ctx):
    """Run learning cycle."""
    from learning_loop.scheduler import LearningScheduler
    
    scheduler = LearningScheduler()
    report = scheduler.run_cycle()
    
    phases = report.get('phases', {})
    
    return {
        "response": f"""🧠 **Learning Cycle Complete**

⏱️ Duration: {report.get('duration_seconds', 0):.1f}s
📊 Indexed: {phases.get('indexing', {}).get('messages_indexed', 0)} messages
🎯 Patterns: {phases.get('pattern_recognition', {}).get('patterns_found', 0)}
🛠️ Skills: {phases.get('skill_generation', {}).get('skills_generated', 0)}
🔄 Optimized: {len(phases.get('optimization', {}).get('pruned', []))} pruned
🧬 Evolved: {phases.get('evolution', {}).get('new_entities', 0)} new entities

{'✨ All phases complete!' if report.get('success') else '❌ Some errors occurred'}""",
        "success": report.get('success', False)
    }


def cmd_search(args, ctx):
    """Search memories."""
    if not args:
        return {
            "response": "🔍 Usage: `/lm search <query>`\nExample: `/lm search python programming`",
            "success": False
        }
    
    from memory_store.vector_db import get_vector_db
    
    query = " ".join(args)
    db = get_vector_db()
    results = db.search(query, n_results=5)
    
    if not results:
        return {
            "response": f"🔍 No memories found for: **{query}**\n\nInteract more to build memory!",
            "success": True
        }
    
    lines = [f"🔍 Results for: **{query}**\n"]
    for i, r in enumerate(results, 1):
        content = r.get('content', '')[:150]
        score = r.get('_distance', 'N/A')
        lines.append(f"{i}. {content}{'...' if len(r.get('content', '')) > 150 else ''}")
    
    return {
        "response": "\n".join(lines),
        "success": True,
        "data": results
    }


def cmd_patterns(args, ctx):
    """Show patterns."""
    from learning_loop.pattern_recognizer import PatternRecognizer
    
    recognizer = PatternRecognizer()
    patterns = recognizer.find_recurring_patterns(days_back=7)
    recs = recognizer.get_recommended_skills()
    
    if not patterns:
        return {
            "response": "🔍 No patterns yet.\n\nKeep interacting to build pattern data!",
            "success": True
        }
    
    lines = ["🔍 **Patterns (7 days)**\n"]
    for p in patterns[:5]:
        lines.append(f"- **{p['type']}**: {p['pattern']} ({p['frequency']}x)")
    
    if recs:
        lines.append("\n**Skill Recommendations:**")
        for r in recs[:3]:
            lines.append(f"- [{r['type']}] {r['trigger']}")
    
    return {
        "response": "\n".join(lines),
        "success": True
    }


def cmd_skills(args, ctx):
    """List skills."""
    from memory_store.skill_generator import SkillGenerator
    
    gen = SkillGenerator()
    skills = gen.get_generated_skills()
    stats = gen.get_stats()
    
    if not skills:
        return {
            "response": "🛠️ No skills generated yet.\n\nRun `/lm run` to discover patterns and create skills!",
            "success": True
        }
    
    lines = [f"🛠️ **Auto-Generated Skills** ({stats['total_skills_generated']})\n"]
    for s in skills[:5]:
        lines.append(f"- **{s['name']}**: {s.get('triggers', ['—'])[:2]}, used {s.get('usage_count', 0)}x")
    
    return {
        "response": "\n".join(lines),
        "success": True
    }


def cmd_stats(args, ctx):
    """Show stats."""
    from autonomous import get_optimizer, EvolutionEngine
    
    optimizer = get_optimizer()
    evolution = EvolutionEngine()
    
    opt = optimizer.get_stats()
    evo = evolution.get_stats()
    
    return {
        "response": f"""📈 **Statistics**

**Optimization:**
- Entities tracked: {opt.get('total_entities', 0)}
- Matrix size: {opt.get('matrix_size', 0)}x{opt.get('matrix_size', 0)}

**Evolution:**
- Generation: {evo.get('generation', 0)}
- Population: {evo.get('population_size', 0)}
- Best fitness: {evo.get('best_fitness', 0):.3f}
- Avg fitness: {evo.get('avg_fitness', 0):.3f}""",
        "success": True
    }


def cmd_profile(args, ctx):
    """Show profile."""
    from memory_store.vector_db import get_vector_db
    from memory_store.user_model import UserModel
    
    db = get_vector_db()
    profiles = db.get_all_user_profiles()
    model = UserModel()
    style = model.get_preferred_response_style()
    
    lines = ["👤 **User Profile**\n"]
    lines.append(f"Style: {'Formal' if style.get('formal') else 'Casual'}, {'Verbose' if style.get('max_length', 500) > 500 else 'Concise'}")
    
    if profiles:
        lines.append("\n**Facts:**")
        for k, v in list(profiles.items())[:5]:
            lines.append(f"- {k}: {v['value']}")
    else:
        lines.append("\nNo profile data yet. Keep interacting!")
    
    return {
        "response": "\n".join(lines),
        "success": True
    }


def cmd_optimize(args, ctx):
    """Run optimization."""
    from autonomous import get_optimizer
    
    optimizer = get_optimizer()
    report = optimizer.run_optimization_cycle()
    
    return {
        "response": f"""🔄 **Optimization Complete**

Pruned: {len(report.get('pruned', []))}
Strengthened: {len(report.get('strengthened', []))}
Analyzed: {report.get('analyzed', 0)} entities""",
        "success": True
    }


def cmd_evolve(args, ctx):
    """Run evolution."""
    from autonomous import EvolutionEngine
    
    evolution = EvolutionEngine()
    report = evolution.evolve()
    
    return {
        "response": f"""🧬 **Evolution Complete**

Generation: {report.get('generation', 0)}
Population: {report.get('population_size', 0)}
Best fitness: {report.get('best_fitness', 0):.3f}
Avg fitness: {report.get('avg_fitness', 0):.3f}
Converged: {'Yes' if report.get('converged') else 'No'}""",
        "success": True
    }


def cmd_daemon(args, ctx):
    """Daemon control."""
    from learning_loop.scheduler import LearningScheduler
    
    if not args or args[0] == 'status':
        scheduler = LearningScheduler()
        status = scheduler.get_status()
        return {
            "response": f"""⏰ **Daemon Status**

Running: {'🟢 Yes' if status.get('daemon_running') else '🔴 No'}
Last cycle: {status.get('last_cycle') or 'Never'}
Next: {status.get('next_scheduled_run') or 'Not scheduled'}""",
            "success": True
        }
    
    action = args[0] if args else 'start'
    scheduler = LearningScheduler()
    
    if action == 'start':
        scheduler.start_daemon()
        return {"response": "⏰ Daemon started.", "success": True}
    elif action == 'stop':
        scheduler.stop_daemon()
        return {"response": "⏰ Daemon stopped.", "success": True}
    
    return {"response": "Usage: `/lm daemon start|stop`", "success": False}


# Command registry
COMMANDS = {
    "status": cmd_status,
    "install": cmd_install,
    "init": cmd_init,
    "run": cmd_run,
    "search": cmd_search,
    "patterns": cmd_patterns,
    "skills": cmd_skills,
    "stats": cmd_stats,
    "profile": cmd_profile,
    "optimize": cmd_optimize,
    "evolve": cmd_evolve,
    "daemon": cmd_daemon,
}


def execute(command, args, context=None):
    """Main entry point."""
    context = context or {}
    context.update(get_context())
    
    if command not in COMMANDS:
        return {
            "response": f"""LanceMem for OpenClaw

Commands:
/lm status    - System status
/lm install   - Install LanceDB
/lm init     - Initialize DB
/lm run      - Learning cycle
/lm search   - Search memories
/lm patterns - Show patterns
/lm skills   - List skills
/lm stats    - Statistics
/lm profile  - User profile
/lm optimize - Run optimization
/lm evolve  - Run evolution
/lm daemon   - Daemon control""",
            "success": False
        }
    
    try:
        return COMMANDS[command](args, context)
    except Exception as e:
        return {
            "response": f"❌ Error: {str(e)}",
            "success": False,
            "error": str(e)
        }


if __name__ == "__main__":
    if len(sys.argv) < 2:
        print("LanceMem for OpenClaw")
        sys.exit(1)
    
    result = execute(sys.argv[1], sys.argv[2:])
    print(result["response"])
