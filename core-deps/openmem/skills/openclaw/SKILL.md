# LanceMem for OpenClaw

_Dedicated skill for full autonomous memory integration with OpenClaw._

## Installation

### Option 1: Automatic (via OpenClaw)

```
/learn install
/learn init
```

### Option 2: Manual

```bash
# Copy to OpenClaw skills directory
cp -r F:\openmem\skills\openclaw ~/.openclaw/skills/

# Or create symlink
mklink /d "%USERPROFILE%\.openclaw\skills\lancemem" "F:\openmem\skills\openclaw"
```

## Commands

| Command | Description |
|---------|-------------|
| `/lancemem` or `/lm` | Show help |
| `/lm status` | System status |
| `/lm install` | Install LanceDB |
| `/lm init` | Initialize database |
| `/lm run` | Run learning cycle |
| `/lm search <q>` | Search memories |
| `/lm patterns` | Show patterns |
| `/lm skills` | List skills |
| `/lm stats` | Statistics |
| `/lm profile` | User profile |
| `/lm optimize` | Run optimization |
| `/lm evolve` | Run evolution |
| `/lm daemon start` | Start daemon |
| `/lm daemon stop` | Stop daemon |

## Quick Usage

```
/lm install    # First time only
/lm init       # First time only
/lm run        # Run learning
/lm status     # Check health
/lm search python  # Find Python memories
```

## Memory Context

When responding, the system automatically:

1. **Indexes** your message to LanceDB
2. **Searches** relevant past memories
3. **Injects** context into response generation
4. **Learns** from the interaction
5. **Optimizes** performance matrix
6. **Evolves** skills as needed

## OpenClaw Skill Structure

```
skills/openclaw/
├── SKILL.md        # This file
├── learner.py      # Command handlers
└── manifest.json   # OpenClaw manifest
```

## Examples

### First Time Setup
```
You: /lm install
Bot: Installing LanceDB...

You: /lm init
Bot: Database initialized. Ready!

You: /lm run
Bot: Learning cycle complete. 15 memories indexed.
```

### Memory Search
```
You: /lm search my project name
Bot: Found 3 relevant memories:
1. You mentioned "Project Alpha" on April 1st
2. Your current project is "OpenMem integration"
3. You want to finish by April 15th
```

### Status Check
```
You: /lm status
Bot: LanceMem Status:
- LanceDB: Connected
- Memories: 247 stored
- Skills: 5 auto-generated
- Population: 20 evolving
- Best fitness: 0.847
- Daemon: Running
```

## Configuration

No configuration needed — works out of the box.

Optional: Create `F:\openmem\config.json` for custom settings.

## Auto-Operation

Once running, the system operates autonomously:

- 🕐 **Every 2 hours**: Learning cycle runs
- 📊 **Every cycle**: Patterns discovered, skills generated
- 🔄 **Every cycle**: Performance matrix optimized
- 🧬 **Every cycle**: Evolution algorithm runs
- 🗑️ **Weekly**: Memory consolidation (daily → weekly → long-term)

## Troubleshooting

```
/lm status          # Check what's wrong
/lm install --force  # Reinstall LanceDB
/lm run --full      # Full re-index
```

## Files

- `learner.py` — All command handlers
- `manifest.json` — OpenClaw skill manifest
- `SKILL.md` — This documentation
