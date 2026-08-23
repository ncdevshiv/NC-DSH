#!/usr/bin/env python3
"""
OpenMem Learner for OpenClaw Skill Integration.

This module provides the OpenClaw skill interface to OpenMem's learning capabilities.

Usage as OpenClaw skill:
    /learn run      - Run a full learning cycle
    /learn status   - Show learning system status
    /learn patterns - Show discovered patterns
    /learn skills   - List auto-generated skills
    /learn search <query> - Search memory
    /learn stats    - Show statistics
    /learn profile  - Show user profile
    /learn schedule - Show scheduling info
"""

import os
import sys
import json
from datetime import datetime
from pathlib import Path

# Add openmem to path
OPENMEM_BASE = Path(__file__).parent.parent.parent
sys.path.insert(0, str(OPENMEM_BASE))

from memory_store import get_vector_db
from memory_store.memory_manager import MemoryManager
from memory_store.user_model import UserModel
from memory_store.skill_generator import SkillGenerator
from learning_loop.conversation_indexer import ConversationIndexer
from learning_loop.pattern_recognizer import PatternRecognizer
from learning_loop.reflection_engine import ReflectionEngine
from learning_loop.scheduler import LearningScheduler


def get_skill_context() -> dict:
    """Get context for skill execution."""
    return {
        "skill_name": "self-learning",
        "skill_version": "1.0.0",
        "openmem_path": str(OPENMEM_BASE),
        "timestamp": datetime.now().isoformat()
    }


def cmd_run(args: list, context: dict) -> dict:
    """Run a full learning cycle."""
    scheduler = LearningScheduler()
    
    report = scheduler.run_cycle()
    
    return {
        "response": f"""🧠 **Learning Cycle Complete**

⏱️ Duration: {report.get('duration_seconds', 0):.1f}s
📊 Messages indexed: {report.get('phases', {}).get('indexing', {}).get('messages_indexed', 0)}
🎯 Patterns found: {report.get('phases', {}).get('pattern_recognition', {}).get('patterns_found', 0)}
🛠️ Skills generated: {report.get('phases', {}).get('skill_generation', {}).get('skills_generated', 0)}
✅ Improvements made: {report.get('phases', {}).get('reflection', {}).get('improvements_completed', 0)}

{('❌ Error: ' + report.get('error', '')) if not report.get('success') else '✨ All phases completed successfully!'}""",
        "success": report.get("success", False),
        "data": report
    }


def cmd_status(args: list, context: dict) -> dict:
    """Show learning system status."""
    scheduler = LearningScheduler()
    status = scheduler.get_status()
    
    indexer = ConversationIndexer()
    index_stats = indexer.get_stats()
    
    vector_db = get_vector_db()
    db_stats = vector_db.get_stats()
    
    response = f"""📊 **OpenMem Status**

**Scheduler:**
- Daemon running: {'🟢 Yes' if status['daemon_running'] else '🔴 No'}
- Last cycle: {status.get('last_cycle', 'Never') or 'Never'}
- Cycles completed: {status.get('cycles_completed', 0)}
- Next run: {status.get('next_scheduled_run', 'N/A')}

**Vector DB:**
- Total memories: {db_stats.get('total_memories', 0)}
- Embedder: {db_stats.get('embedder', 'unknown')}
- User profiles: {db_stats.get('total_user_profiles', 0)}

**Indexing:**
- Sessions indexed: {index_stats.get('index_state', {}).get('total_indexed', 0)}
- Total messages: {index_stats.get('index_state', {}).get('total_messages', 0)}

**Overall Stats:**
- Messages indexed: {status.get('stats', {}).get('total_messages_indexed', 0)}
- Skills generated: {status.get('stats', {}).get('skills_generated', 0)}
- Improvements made: {status.get('stats', {}).get('improvements_made', 0)}"""
    
    return {
        "response": response,
        "success": True,
        "data": status
    }


def cmd_patterns(args: list, context: dict) -> dict:
    """Show discovered patterns."""
    recognizer = PatternRecognizer()
    patterns = recognizer.find_recurring_patterns(days_back=7)
    recommendations = recognizer.get_recommended_skills()
    
    if not patterns:
        return {
            "response": "🔍 No significant patterns discovered yet. Keep using the system to build pattern data!",
            "success": True,
            "data": []
        }
    
    pattern_lines = []
    for i, p in enumerate(patterns[:10], 1):
        pattern_lines.append(f"{i}. **{p['type']}**: {p['pattern']} (seen {p['frequency']}x, confidence {p.get('confidence', 0):.0%})")
        if p.get('recommendation'):
            pattern_lines.append(f"   → {p['recommendation']}")
    
    rec_lines = []
    for i, r in enumerate(recommendations[:5], 1):
        rec_lines.append(f"{i}. [{r['type']}] {r['trigger']} (freq: {r['frequency']})")
    
    response = f"""🔍 **Pattern Analysis (7 days)**

**Top Patterns:**
{chr(10).join(pattern_lines)}

**Skill Recommendations:**
{chr(10).join(rec_lines) if rec_lines else 'None yet — patterns will suggest skill generation.'}"""
    
    return {
        "response": response,
        "success": True,
        "data": {"patterns": patterns, "recommendations": recommendations}
    }


def cmd_skills(args: list, context: dict) -> dict:
    """List auto-generated skills."""
    generator = SkillGenerator()
    skills = generator.get_generated_skills()
    stats = generator.get_stats()
    
    if not skills:
        return {
            "response": "🛠️ No auto-generated skills yet. Run `/learn run` to discover patterns and generate skills!",
            "success": True,
            "data": []
        }
    
    skill_lines = []
    for s in skills:
        skill_lines.append(f"- **{s['name']}**: {s.get('triggers', ['unknown'])[:3]}, used {s.get('usage_count', 0)}x")
    
    response = f"""🛠️ **Auto-Generated Skills** ({stats['total_skills_generated']} total)

{chr(10).join(skill_lines)}

Total uses: {stats['total_uses']}"""
    
    return {
        "response": response,
        "success": True,
        "data": skills
    }


def cmd_search(args: list, context: dict) -> dict:
    """Semantic search over memory."""
    if not args:
        return {
            "response": "🔍 Usage: `/learn search <query>`\nExample: `/learn search what is the user's project name`",
            "success": False,
            "error": "No query provided"
        }
    
    query = " ".join(args)
    vector_db = get_vector_db()
    results = vector_db.search(query, n_results=5)
    
    if not results:
        return {
            "response": f"🔍 No memories found for: **{query}**",
            "success": True,
            "data": []
        }
    
    result_lines = []
    for i, r in enumerate(results, 1):
        tier = r.get("metadata", {}).get("tier", "memory")
        result_lines.append(f"{i}. [{tier}] {r['content'][:150]}{'...' if len(r['content']) > 150 else ''}")
    
    response = f"""🔍 **Memory Search: "{query}"**

{chr(10).join(result_lines)}"""
    
    return {
        "response": response,
        "success": True,
        "data": results
    }


def cmd_stats(args: list, context: dict) -> dict:
    """Show memory system statistics."""
    memory_manager = MemoryManager()
    stats = memory_manager.get_stats()
    
    vector_db = get_vector_db()
    db_stats = vector_db.get_stats()
    
    tier_info = stats.get("tiers", {})
    
    response = f"""📈 **Memory Statistics**

**Memory Tiers:**
- Daily memories: {tier_info.get('daily', {}).get('count', 0)}
- Weekly summaries: {tier_info.get('weekly', {}).get('count', 0)}
- Long-term memories: {tier_info.get('longterm', {}).get('count', 0)}

**Vector DB:**
- Total embeddings: {db_stats.get('total_memories', 0)}
- Avg importance: {db_stats.get('avg_importance', 0):.2f}
- Embedder type: {db_stats.get('embedder', 'unknown')}

**User Profiles:**
- Facts tracked: {db_stats.get('total_user_profiles', 0)}"""
    
    return {
        "response": response,
        "success": True,
        "data": stats
    }


def cmd_profile(args: list, context: dict) -> dict:
    """Show user profile."""
    user_model = UserModel()
    profile_summary = user_model.get_profile_summary()
    profile_context = user_model.get_context_for_new_interaction()
    modeling_stats = user_model.get_stats()
    
    response = f"""👤 **User Profile**

{profile_summary}

**Topics of Interest:** {', '.join(user_model.profile.get('topics_of_interest', [])[:5]) or 'Learning...'}

**Communication Style:**
- Formality: {user_model.profile['communication_style']['formality']:.0%}
- Verbosity: {user_model.profile['communication_style']['verbosity']:.0%}
- Emoji usage: {user_model.profile['communication_style']['emoji_usage']:.0%}

**Tracked Facts:**
{chr(10).join([f'- {k}: {v["value"]} ({v["confidence"]:.0%} confidence)' for k, v in user_model.profile.get('important_facts', {}).items()]) or 'None yet'}"""
    
    return {
        "response": response,
        "success": True,
        "data": profile_context
    }


def cmd_schedule(args: list, context: dict) -> dict:
    """Show scheduling info."""
    scheduler = LearningScheduler()
    status = scheduler.get_status()
    
    response = f"""⏰ **Learning Schedule**

- Interval: Every {scheduler.config['interval_hours']} hours
- Daemon: {'🟢 Running' if status['daemon_running'] else '🔴 Stopped'}
- Last run: {status.get('last_cycle') or 'Never'}
- Next run: {status.get('next_scheduled_run') or 'Not scheduled'}

**To start daemon:**
`python main.py daemon start`

**To run once:**
`python main.py run-cycle`"""
    
    return {
        "response": response,
        "success": True,
        "data": status
    }


# Command registry
COMMANDS = {
    "run": cmd_run,
    "status": cmd_status,
    "patterns": cmd_patterns,
    "skills": cmd_skills,
    "search": cmd_search,
    "stats": cmd_stats,
    "profile": cmd_profile,
    "schedule": cmd_schedule,
}


def execute(command: str, args: list, context: dict = None) -> dict:
    """
    Main entry point for the OpenClaw skill.
    
    Args:
        command: The subcommand (run, status, search, etc.)
        args: Arguments for the command
        context: OpenClaw context dict (optional)
        
    Returns:
        dict with 'response' (str), 'success' (bool), optional 'data' (dict)
    """
    context = context or {}
    context.update(get_skill_context())
    
    if command not in COMMANDS:
        available = ", ".join(COMMANDS.keys())
        return {
            "response": f"❓ Unknown command: {command}\n\nAvailable commands: {available}\n\nUsage:\n/learn run - Run learning cycle\n/learn status - Show status\n/learn patterns - Show patterns\n/learn skills - List skills\n/learn search <query> - Search memory\n/learn stats - Show stats\n/learn profile - Show user profile",
            "success": False,
            "error": f"Unknown command: {command}"
        }
    
    try:
        return COMMANDS[command](args, context)
    except Exception as e:
        import traceback
        return {
            "response": f"❌ Error in /learn {command}: {str(e)}\n\n_{traceback.format_exc()}_",
            "success": False,
            "error": str(e)
        }


# CLI entry point
if __name__ == "__main__":
    import sys
    
    if len(sys.argv) < 2:
        print("OpenMem Learner for OpenClaw")
        print("Usage: python learner.py <command> [args]")
        print("Commands: run, status, patterns, skills, search, stats, profile, schedule")
        sys.exit(1)
    
    command = sys.argv[1]
    args = sys.argv[2:]
    
    result = execute(command, args)
    print(result["response"])
    sys.exit(0 if result.get("success") else 1)
