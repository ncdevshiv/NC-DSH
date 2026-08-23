# OpenMem — VS Code Memory Skill

_Autonomous memory for VS Code, powered by OpenMem._

## Commands

| Command | Description |
|---------|-------------|
| `/mem` or `/mem help` | Show help |
| `/mem status` | System status |
| `/mem search <query>` | Search memories |
| `/mem run` | Run learning cycle |
| `/mem patterns` | Show discovered patterns |
| `/mem skills` | List auto-generated skills |
| `/mem stats` | Statistics |
| `/mem profile` | User profile |
| `/mem optimize` | Run optimization |
| `/mem evolve` | Run evolution |

## Quick Start

```
/mem status     # Check health
/mem run       # Run learning cycle
/mem search python  # Find Python memories
```

## How It Works

1. **Indexing**: Every conversation is automatically indexed to LanceDB
2. **Context**: Memory context is injected into `.vscode/memory.md`
3. **Search**: Semantic vector search finds relevant past conversations
4. **Learning**: Patterns are discovered, skills auto-generated
5. **Evolution**: Response strategies evolve over time

## Installation

```bash
# From OpenMem root
python bin/launcher.py --install
python bin/launcher.py --skill vscode
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
- `learner.py` — Command handler for VS Code
- `config.json` — Agent-specific configuration
