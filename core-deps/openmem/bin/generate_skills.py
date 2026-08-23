#!/usr/bin/env python3
"""Generate skill files for all agent adapters.

Regeneration contract: every file under agents/<agent>/skill/ is generated
output of this module. Never edit those artifacts by hand — change the
templates here and re-run ``python bin/generate_skills.py``. Output is
byte-deterministic (no timestamps, explicit CRLF) so that committed files
always equal generator output; tests/test_skill_consistency.py enforces it.
"""

import os
import json
import sys

AGENTS = {
    "qwen_code": {
        "display": "Qwen Code",
        "trigger": "/mem",
        "context_file": ".qwen/memory_context.md",
        "workspace_env": "QWEN_CODE_WORKSPACE",
    },
    "claude_code": {
        "display": "Claude Code",
        "trigger": "/mem",
        "context_file": "CLAUDE.md",
        "workspace_env": "CLAUDE_CODE_WORKSPACE",
    },
    "codex_cli": {
        "display": "Codex CLI",
        "trigger": "/mem",
        "context_file": ".codex/context.md",
        "workspace_env": "CODEX_WORKSPACE",
    },
    "opencode": {
        "display": "OpenCode",
        "trigger": "/memory",
        "context_file": ".opencode/context.md",
        "workspace_env": "OPENCODE_WORKSPACE",
    },
    "antigravity_ide": {
        "display": "Antigravity IDE",
        "trigger": "/mem",
        "context_file": ".antigravity/memory.md",
        "workspace_env": "ANTIGRAVITY_WORKSPACE",
    },
    "kilo_cli": {
        "display": "Kilo CLI",
        "trigger": "/mem",
        "context_file": ".kilo/context.md",
        "workspace_env": "KILO_WORKSPACE",
    },
    "vscode": {
        "display": "VS Code",
        "trigger": "/mem",
        "context_file": ".vscode/memory.md",
        "workspace_env": "VSCODE_CWD",
    },
    "windsurf": {
        "display": "Windsurf",
        "trigger": "@memory",
        "context_file": ".windsurf/memory.md",
        "workspace_env": "WINDSURF_WORKSPACE",
    },
    "cursor": {
        "display": "Cursor",
        "trigger": "@memory",
        "context_file": ".cursor/rules/memory.md",
        "workspace_env": "CURSOR_WORKSPACE",
    },
    "openclaw": {
        "display": "OpenClaw",
        "trigger": "/lm",
        "context_file": "~/.openclaw/memory_context.md",
        "workspace_env": "",
    },
    # Fallback adapter (agents/generic/adapter.py): sessions come from
    # GENERIC_SESSION_DIR or <workspace>/.sessions/; when neither exists the
    # adapter creates <workspace>/.openmem/sessions/ and inject_context()
    # writes _memory_context.txt into that session dir — hence context_file.
    # GENERIC_SESSION_DIR is the documented env hook (README.md "Generic").
    # base.auto_detect_adapter() falls back to Generic, and base.install_skill
    # derives this dir name from AGENT_NAME "Generic" -> agents/generic/skill.
    "generic": {
        "display": "Any Agent",
        "trigger": "/mem",
        "context_file": ".openmem/sessions/_memory_context.txt",
        "workspace_env": "GENERIC_SESSION_DIR",
    },
}

BASE_DIR = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def make_skill_md(agent_key, agent_info):
    return f'''# OpenMem — {agent_info["display"]} Memory Skill

_Autonomous memory for {agent_info["display"]}, powered by OpenMem._

## Commands

| Command | Description |
|---------|-------------|
| `{agent_info["trigger"]}` or `{agent_info["trigger"]} help` | Show help |
| `{agent_info["trigger"]} status` | System status |
| `{agent_info["trigger"]} search <query>` | Search memories |
| `{agent_info["trigger"]} run` | Run learning cycle |
| `{agent_info["trigger"]} patterns` | Show discovered patterns |
| `{agent_info["trigger"]} skills` | List auto-generated skills |
| `{agent_info["trigger"]} stats` | Statistics |
| `{agent_info["trigger"]} profile` | User profile |
| `{agent_info["trigger"]} optimize` | Run optimization |
| `{agent_info["trigger"]} evolve` | Run evolution |

## Quick Start

```
{agent_info["trigger"]} status     # Check health
{agent_info["trigger"]} run       # Run learning cycle
{agent_info["trigger"]} search python  # Find Python memories
```

## How It Works

1. **Indexing**: Every conversation is automatically indexed to LanceDB
2. **Context**: Memory context is injected into `{agent_info["context_file"]}`
3. **Search**: Semantic vector search finds relevant past conversations
4. **Learning**: Patterns are discovered, skills auto-generated
5. **Evolution**: Response strategies evolve over time

## Installation

```bash
# From OpenMem root
python bin/launcher.py --install
python bin/launcher.py --skill {agent_key}
```

## Auto-Operation

Once initialized, OpenMem operates autonomously:
- 🕐 Every 2 hours: Learning cycle
- 📊 Every cycle: Pattern discovery, skill generation
- 🔄 Every cycle: Performance optimization
- 🧬 Every cycle: Evolution algorithm
- 🗑️ Weekly: Memory consolidation

## Files

- `SKILL.md` — This documentation
- `learner.py` — Command handler for {agent_info["display"]}
- `config.json` — Agent-specific configuration
'''


def make_learner_py(agent_key, agent_info):
    return f'''#!/usr/bin/env python3
"""
OpenMem Memory Skill for {agent_info["display"]}.

Usage:
    {agent_info["trigger"]}          - Show help
    {agent_info["trigger"]} status   - System status
    {agent_info["trigger"]} search <query> - Search memories
    {agent_info["trigger"]} run      - Run learning cycle

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
    return {{
        "agent": "{agent_info["display"]}",
        "agent_key": "{agent_key}",
        "skill_version": "1.0.0",
        "timestamp": datetime.now().isoformat(),
    }}


def cmd_status(args, context):
    """Show OpenMem status."""
    try:
        from memory_store.vector_db import get_vector_db
        from learning_loop.scheduler import LearningScheduler

        db = get_vector_db()
        stats = db.get_stats()
        scheduler = LearningScheduler()
        sched = scheduler.get_status()

        return {{
            "response": f"""📊 **OpenMem Status ({agent_info["display"]})**

**Database:**
- Tables: {{', '.join(stats.get('tables', ['none']))}}
- Memories: {{stats.get('table_memories_rows', 0)}}

**Scheduler:**
- Last cycle: {{sched.get('last_cycle', 'Never')}}
- Cycles: {{sched.get('cycles_completed', 0)}}""",
            "success": True,
        }}
    except Exception as e:
        return {{"response": f"❌ Error: {{e}}", "success": False}}


def cmd_search(args, context):
    """Search memories."""
    if not args:
        return {{"response": "Usage: {agent_info["trigger"]} search <query>", "success": False}}

    try:
        from memory_store.vector_db import get_vector_db
        db = get_vector_db()
        query = " ".join(args)
        results = db.search(query, n_results=5)

        if not results:
            return {{"response": f"🔍 No memories found for: **{{query}}**", "success": True}}

        lines = [f"🔍 **Search: \\"{{query}}\\"**\\n"]
        for i, r in enumerate(results, 1):
            content = r.get("content", "")[:200]
            lines.append(f"{{i}}. {{content}}")
            if r.get("metadata", {{}}).get("tier"):
                lines.append(f"   [{{r['metadata']['tier']}}] importance: {{r.get('importance', 0):.2f}}")

        return {{"response": "\\n".join(lines), "success": True}}
    except Exception as e:
        return {{"response": f"❌ Error: {{e}}", "success": False}}


def cmd_run(args, context):
    """Run learning cycle."""
    try:
        from learning_loop.scheduler import LearningScheduler
        scheduler = LearningScheduler()
        report = scheduler.run_cycle()

        phases = report.get("phases", {{}})
        indexing = phases.get("indexing", {{}})
        patterns = phases.get("pattern_recognition", {{}})
        skills = phases.get("skill_generation", {{}})

        return {{
            "response": f"""🧠 **Learning Cycle Complete**

⏱️ Duration: {{report.get('duration_seconds', 0):.1f}}s
📊 Messages indexed: {{indexing.get('messages_indexed', 0)}}
🎯 Patterns found: {{patterns.get('patterns_found', 0)}}
🛠️ Skills generated: {{skills.get('skills_generated', 0)}}""",
            "success": report.get("success", False),
        }}
    except Exception as e:
        return {{"response": f"❌ Error: {{e}}", "success": False}}


def cmd_patterns(args, context):
    """Show patterns."""
    try:
        from learning_loop.pattern_recognizer import PatternRecognizer
        pr = PatternRecognizer()
        patterns = pr.find_recurring_patterns(days_back=7)

        if not patterns:
            return {{"response": "🔍 No patterns discovered yet.", "success": True}}

        lines = ["🔍 **Discovered Patterns**\\n"]
        for i, p in enumerate(patterns[:10], 1):
            lines.append(f"{{i}}. **{{p['type']}}**: {{p['pattern']}} (freq: {{p['frequency']}}, conf: {{p.get('confidence', 0):.0%}})")

        return {{"response": "\\n".join(lines), "success": True}}
    except Exception as e:
        return {{"response": f"❌ Error: {{e}}", "success": False}}


def cmd_skills(args, context):
    """List skills."""
    try:
        from memory_store.skill_generator import SkillGenerator
        gen = SkillGenerator()
        skills = gen.get_generated_skills()

        if not skills:
            return {{"response": "🛠️ No auto-generated skills yet.", "success": True}}

        lines = ["🛠️ **Auto-Generated Skills**\\n"]
        for s in skills:
            lines.append(f"- **{{s.get('name', 'unknown')}}**: triggers: {{s.get('triggers', [])[:3]}}")

        return {{"response": "\\n".join(lines), "success": True}}
    except Exception as e:
        return {{"response": f"❌ Error: {{e}}", "success": False}}


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

        return {{
            "response": f"""📈 **OpenMem Statistics**

**Database:**
- Tables: {{len(stats.get('tables', []))}}

**Learning:**
- Cycles: {{sched.get('cycles_completed', 0)}}
- Messages indexed: {{sched.get('stats', {{}}).get('total_messages_indexed', 0)}}

**Optimization:**
- Entities: {{opt.get('total_entities', 0)}}

**Evolution:**
- Generation: {{evo.get('generation', 0)}}
- Best fitness: {{evo.get('best_fitness', 0):.3f}}""",
            "success": True,
        }}
    except Exception as e:
        return {{"response": f"❌ Error: {{e}}", "success": False}}


def cmd_profile(args, context):
    """Show user profile."""
    try:
        from memory_store.vector_db import get_vector_db
        db = get_vector_db()
        profiles = db.get_all_user_profiles()

        if not profiles:
            return {{"response": "👤 No profile data yet.", "success": True}}

        lines = ["👤 **User Profile**\\n"]
        for key, data in profiles.items():
            lines.append(f"- **{{key}}**: {{data['value']}} ({{data['confidence']:.0%}})")

        return {{"response": "\\n".join(lines), "success": True}}
    except Exception as e:
        return {{"response": f"❌ Error: {{e}}", "success": False}}


def cmd_optimize(args, context):
    """Run optimization."""
    try:
        from autonomous import get_optimizer
        optimizer = get_optimizer()
        report = optimizer.run_optimization_cycle()
        return {{
            "response": f"🔄 **Optimization Complete**\\n\\nPruned: {{len(report.get('pruned', []))}}\\nStrengthened: {{len(report.get('strengthened', []))}}",
            "success": True,
        }}
    except Exception as e:
        return {{"response": f"❌ Error: {{e}}", "success": False}}


def cmd_evolve(args, context):
    """Run evolution."""
    try:
        from autonomous import EvolutionEngine
        engine = EvolutionEngine()
        report = engine.evolve()
        return {{
            "response": f"🧬 **Evolution Complete**\\n\\nGeneration: {{report.get('generation', 0)}}\\nBest fitness: {{report.get('best_fitness', 0):.3f}}",
            "success": True,
        }}
    except Exception as e:
        return {{"response": f"❌ Error: {{e}}", "success": False}}


COMMANDS = {{
    "status": cmd_status,
    "search": cmd_search,
    "run": cmd_run,
    "patterns": cmd_patterns,
    "skills": cmd_skills,
    "stats": cmd_stats,
    "profile": cmd_profile,
    "optimize": cmd_optimize,
    "evolve": cmd_evolve,
    "help": lambda a, c: {{
        "response": f"""📖 **OpenMem Help ({agent_info["display"]})**

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
    }},
}}


def execute(command, args=None, context=None):
    """Main entry point."""
    args = args or []
    context = context or {{}}
    context.update(get_context())

    if command not in COMMANDS:
        return COMMANDS["help"](args, context)

    try:
        return COMMANDS[command](args, context)
    except Exception as e:
        import traceback
        return {{
            "response": f"❌ Error: {{e}}\\n\\n_{{traceback.format_exc()}}_",
            "success": False,
        }}


if __name__ == "__main__":
    if len(sys.argv) < 2:
        result = COMMANDS["help"]([], {{}})
    else:
        result = execute(sys.argv[1], sys.argv[2:])
    print(result["response"])
    sys.exit(0 if result.get("success") else 1)
'''


def make_config_json(agent_key, agent_info):
    # No "generated_at" timestamp: artifacts must be byte-reproducible so the
    # committed files always equal this module's output. Nothing consumes
    # skill-level config.json keys at runtime (adapters only copy these files).
    return {
        "agent": agent_info["display"],
        "agent_key": agent_key,
        "trigger": agent_info["trigger"],
        "context_file": agent_info["context_file"],
        "workspace_env": agent_info.get("workspace_env", ""),
        "auto_memory": True,
        "auto_learn": True,
        "learn_interval_hours": 2,
        "max_search_results": 5,
        "min_importance_threshold": 0.3,
        "version": "1.0.0",
    }


# The three artifacts every agent's skill directory must contain.
ARTIFACT_FILENAMES = ("SKILL.md", "learner.py", "config.json")


def build_artifacts(agent_key):
    """Build raw artifact contents for one registered agent.

    Args:
        agent_key: Key into AGENTS (e.g. "qwen_code", "generic")

    Returns:
        Dict mapping artifact filename to its content string (LF line endings)
    """
    info = AGENTS[agent_key]
    return {
        "SKILL.md": make_skill_md(agent_key, info),
        "learner.py": make_learner_py(agent_key, info),
        "config.json": json.dumps(make_config_json(agent_key, info), indent=2),
    }


def render_bytes(agent_key):
    """Serialize an agent's artifacts to the exact bytes written on disk.

    Line endings are normalized to CRLF explicitly so output is identical
    regardless of the platform the generator runs on.

    Args:
        agent_key: Key into AGENTS

    Returns:
        Dict mapping artifact filename to UTF-8 encoded file bytes
    """
    return {
        name: content.replace("\n", "\r\n").encode("utf-8")
        for name, content in build_artifacts(agent_key).items()
    }


def main():
    # Status lines contain non-ASCII glyphs; keep them from crashing under
    # legacy Windows console codepages (cp1252/cp437) when stdout is piped.
    try:
        sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    except (AttributeError, ValueError):
        pass

    base = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

    for agent_key, agent_info in AGENTS.items():
        skill_dir = os.path.join(base, "agents", agent_key, "skill")
        os.makedirs(skill_dir, exist_ok=True)

        artifacts = render_bytes(agent_key)
        for fname in ARTIFACT_FILENAMES:
            with open(os.path.join(skill_dir, fname), "wb") as f:
                f.write(artifacts[fname])

        print(f"✅ Generated skills for {agent_info['display']}")

    print(f"\nDone! Generated skills for {len(AGENTS)} agents.")


if __name__ == "__main__":
    main()
