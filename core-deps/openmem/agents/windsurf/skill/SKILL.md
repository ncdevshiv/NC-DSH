# OpenMem — Windsurf Memory Skill

_Autonomous memory for Windsurf, powered by OpenMem._

## Commands

| Command | Description |
|---------|-------------|
| `@memory` or `@memory help` | Show help |
| `@memory status` | System status |
| `@memory search <query>` | Search memories |
| `@memory run` | Run learning cycle |
| `@memory patterns` | Show discovered patterns |
| `@memory skills` | List auto-generated skills |
| `@memory stats` | Statistics |
| `@memory profile` | User profile |
| `@memory optimize` | Run optimization |
| `@memory evolve` | Run evolution |

## Quick Start

```
@memory status     # Check health
@memory run       # Run learning cycle
@memory search python  # Find Python memories
```

## How It Works

1. **Indexing**: Every conversation is automatically indexed to LanceDB
2. **Context**: Memory context is injected into `.windsurf/memory.md`
3. **Search**: Semantic vector search finds relevant past conversations
4. **Learning**: Patterns are discovered, skills auto-generated
5. **Evolution**: Response strategies evolve over time

## Installation

```bash
# From OpenMem root
python bin/launcher.py --install
python bin/launcher.py --skill windsurf
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
- `learner.py` — Command handler for Windsurf
- `config.json` — Agent-specific configuration
