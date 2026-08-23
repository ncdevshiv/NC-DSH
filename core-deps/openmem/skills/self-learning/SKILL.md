# Self-Learning (OpenMem Integration)

_Built with OpenMem — bridges OpenClaw with Hermes-style autonomous learning._

## What This Skill Does

This skill brings **autonomous learning capabilities** to OpenClaw, inspired by Hermes Agent's self-improvement loop:

- 🔍 **Semantic memory search** — Find relevant context from all past conversations
- 🧠 **Auto user modeling** — Learns user preferences, style, and facts automatically
- 📦 **Skill auto-generation** — Creates new skills from recurring successful patterns
- 🔄 **Self-reflection** — Evaluates interactions and identifies improvements
- 📅 **Memory consolidation** — Distills daily → weekly → long-term memory

## Usage

### Commands

```
/learn run          # Run a full learning cycle now
/learn status       # Show learning system status
/learn patterns     # Show discovered patterns
/learn skills       # List auto-generated skills
/learn search <query>  # Semantic search over memory
/learn stats         # Show memory system statistics
```

### Automatic Behavior

The learning system also runs automatically on a schedule (every 2 hours by default):

1. Indexes recent conversations
2. Recognizes patterns in user requests
3. Generates skills for recurring topics
4. Reflects on interaction quality
5. Consolidates memory tiers

## Context Variables

When this skill is active, these context variables are available:

- `${user_profile_summary}` — Human-readable user profile
- `${preferred_response_style}` — User's preferred response format
- `${active_hours}` — When the user is most active
- `${relevant_memories}` — Memories relevant to current conversation

## Files

- `learner.py` — Main execution logic
- OpenClaw will call this skill based on user commands starting with `/learn`

## Configuration

The skill reads from `F:\openmem\` where OpenMem stores:
- Vector DB data
- Memory tiers (daily/weekly/longterm)
- Generated skills
- User profiles
- Reflection logs

## Hermes Comparison

| Capability | Hermes Agent | OpenClaw + OpenMem |
|---|---|---|
| Learning loop | ✅ Native | ✅ Implemented |
| Skill auto-generation | ✅ | ✅ |
| User modeling | ✅ | ✅ |
| Memory tiers | ✅ | ✅ |
| Self-reflection | ✅ | ✅ |

---

_This skill is the bridge — OpenClaw provides the platform, OpenMem provides the brain._
