# OpenMem — OpenClaw Memory Skill

_Autonomous memory for OpenClaw, powered by OpenMem._

## Commands

| Command | Description |
|---------|-------------|
| `/lm` or `/lm help` | Show help |
| `/lm status` | System status |
| `/lm search <query>` | Search memories |
| `/lm run` | Run learning cycle |
| `/lm patterns` | Show discovered patterns |
| `/lm skills` | List auto-generated skills |
| `/lm stats` | Statistics |
| `/lm profile` | User profile |
| `/lm optimize` | Run optimization |
| `/lm evolve` | Run evolution |

## Quick Start

```
/lm status     # Check health
/lm run       # Run learning cycle
/lm search python  # Find Python memories
```

## How It Works

1. **Indexing**: Every conversation is automatically indexed to LanceDB
2. **Context**: Memory context is injected into `~/.openclaw/memory_context.md`
3. **Search**: Semantic vector search finds relevant past conversations
4. **Learning**: Patterns are discovered, skills auto-generated
5. **Evolution**: Response strategies evolve over time

## Installation

```bash
# From OpenMem root
python bin/launcher.py --install
python bin/launcher.py --skill openclaw
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
- `learner.py` — Command handler for OpenClaw
- `config.json` — Agent-specific configuration
