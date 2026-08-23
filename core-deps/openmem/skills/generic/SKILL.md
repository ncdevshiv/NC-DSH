# LanceMem Generic Agent Skill

_Drop-in autonomous memory for ANY AI agent framework._

## Why This Exists

You have an agent (LangChain, AutoGPT, CrewAI, Custom). You want memory.

**Add this skill.** That's it.

## Installation

### Step 1: Get LanceMem

```bash
cd F:\openmem
pip install -r requirements.txt
python main.py init
```

### Step 2: Import in Your Agent

```python
from skills.generic.lance_mem_agent import LanceMemAgent

# Wrap your agent
agent = LanceMemAgent(your_existing_agent)

# Now your agent has memory!
```

## For LangChain

```python
from langchain.agents import Agent
from skills.generic.lance_mem_agent import LanceMemAgent

# Create LangChain agent
agent = Agent.from_llm_and_tools(...)

# Wrap with memory
memory_agent = LanceMemAgent(agent)

# Use normally - memory is automatic
memory_agent.run("Build me a website")
```

## For AutoGPT /自主Agent

```python
from skills.generic.lance_mem_agent import LanceMemAgent

class MyAutoGPT:
    def __init__(self):
        self.memory = LanceMemAgent(self)
    
    def think(self, task):
        # Memory auto-injects context
        return self.execute(task)

agent = MyAutoGPT()
```

## For CrewAI

```python
from crewai import Agent
from skills.generic.lance_mem_agent import LanceMemAgent

researcher = Agent(role="Researcher")
researcher = LanceMemAgent(researcher)

# CrewAI handles the rest
```

## For Custom Agents

```python
from skills.generic.lance_mem_agent import LanceMemAgent

# Just wrap anything
my_custom_agent = LanceMemAgent(custom_agent_object)

# Get methods added:
# - search(query)
# - add_message(role, content)
# - run_cycle()
# - get_profile()
```

## What You Get

| Capability | Method |
|-----------|--------|
| Semantic search | `agent.search("topic")` |
| Add message | `agent.add_message("user", "hi")` |
| Learning cycle | `agent.run_cycle()` |
| User profile | `agent.get_profile()` |
| Status | `agent.status()` |
| Optimization | `agent.optimize()` |
| Evolution | `agent.evolve()` |

## Auto-Memory Mode

Enable auto-memory for transparent operation:

```python
agent = LanceMemAgent(
    your_agent,
    auto_memory=True,  # Default: True
    auto_learn=True,     # Run cycles automatically
    interval=7200      # Seconds between cycles (2 hours)
)
```

With auto-memory:
1. Every `add_message()` is indexed
2. Every `run()` checks for relevant context
3. Learning cycles run on schedule
4. You don't manage anything

## Architecture

```
┌─────────────────────────────────────────────┐
│              Your Agent                      │
│                                             │
│  agent.think("Build API")                   │
└─────────────────┬───────────────────────────┘
                  │
                  ▼
┌─────────────────────────────────────────────┐
│           LanceMemAgent Wrapper              │
│                                             │
│  1. Search memory for context               │
│  2. Inject into agent prompt                │
│  3. Execute agent                          │
│  4. Record interaction                     │
│  5. Trigger learning if needed              │
└─────────────────────────────────────────────┘
```

## Memory Flow

```
User: "Build me a Python API"
           │
           ▼
┌──────────────────────┐
│ agent.search(query) │
└──────────┬───────────┘
           │
           ▼
┌──────────────────────┐
│ LanceDB Vector Search │
└──────────┬───────────┘
           │
           ▼
    Relevant memories
    - Past API projects
    - User preferences
    - Success patterns
           │
           ▼
┌──────────────────────┐
│ Context Injection     │
│ + Agent Execution     │
└──────────┬───────────┘
           │
           ▼
┌──────────────────────┐
│ agent.add_message()   │
│ + Pattern Learning    │
└──────────────────────┘
```

## Configuration

### Via Constructor

```python
agent = LanceMemAgent(
    my_agent,
    db_path="./my_memory",
    embedding_model="all-MiniLM-L6-v2",
    auto_learn=True,
    learn_interval=3600,  # 1 hour
    min_importance=0.5,
    max_memories=1000
)
```

### Via Config File

Create `config.json` in LanceMem root:

```json
{
  "db_path": "./data/lancedb",
  "embedding_model": "all-MiniLM-L6-v2",
  "auto_learn": true,
  "learn_interval": 7200,
  "optimization": {
    "prune_threshold": 0.2,
    "auto_run": true
  },
  "evolution": {
    "population_size": 20,
    "auto_run": true
  }
}
```

## Complete Example

```python
#!/usr/bin/env python3
"""
Example: LangChain agent with LanceMem memory.
"""

from langchain import OpenAI
from langchain.agents import Agent
from skills.generic.lance_mem_agent import LanceMemAgent

# Create base agent
llm = OpenAI(temperature=0)
agent = Agent.from_llm_and_tools(llm, tools=[...])

# Wrap with memory
memory_agent = LanceMemAgent(
    agent,
    auto_memory=True,
    auto_learn=True
)

# Use normally
response = memory_agent.run("What did I ask about yesterday?")

# That's it! Memory is automatic.
```

## File Structure

```
skills/generic/
├── SKILL.md                    # This file
├── lance_mem_agent.py          # Agent wrapper
├── base_agent.py               # Abstract base
└── adapters/
    ├── langchain_adapter.py    # LangChain integration
    ├── crewai_adapter.py       # CrewAI integration
    └── auto_gpt_adapter.py     # AutoGPT integration
```

## Testing

```bash
# Run tests
python skills/generic/test_adapter.py

# Quick test
python skills/generic/test_agent.py
```

## Portability

This skill is **fully portable**:

- ✅ Windows, Linux, macOS
- ✅ Python 3.8+
- ✅ Any agent framework
- ✅ Cloud or local
- ✅ Single copy-paste

## Next Steps

1. Read `doc/PROMPT_INTEGRATION.md`
2. Copy `skills/generic/` to your agent
3. Import `LanceMemAgent`
4. Wrap your agent
5. Done!

## Support

- Docs: `doc/README.md`
- Integration: `doc/PROMPT_INTEGRATION.md`
- Issues: GitHub
