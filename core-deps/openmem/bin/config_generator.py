#!/usr/bin/env python3
"""
OpenMem Configuration Generator.

Auto-detects agents, generates agent-specific config files,
and writes the master config.json.

Usage:
    python bin/config_generator.py              # Auto-detect + generate
    python bin/config_generator.py --agent qwen # Generate for specific agent
    python bin/config_generator.py --all        # Generate for all agents
    python bin/config_generator.py --show       # Show current config
"""

import os
import sys
import json
from pathlib import Path
from datetime import datetime

BIN_DIR = Path(__file__).parent
OPENMEM_ROOT = BIN_DIR.parent
DATA_DIR = OPENMEM_ROOT / "data"
CONFIG_FILE = OPENMEM_ROOT / "config.json"

SUPPORTED_AGENTS = [
    "qwen_code", "claude_code", "codex_cli", "opencode",
    "antigravity_ide", "kilo_cli", "vscode", "windsurf",
    "cursor", "openclaw"
]

AGENT_DISPLAY = {
    "qwen_code": "Qwen Code",
    "claude_code": "Claude Code",
    "codex_cli": "Codex CLI",
    "opencode": "OpenCode",
    "antigravity_ide": "Antigravity IDE",
    "kilo_cli": "Kilo CLI",
    "vscode": "VS Code",
    "windsurf": "Windsurf",
    "cursor": "Cursor",
    "openclaw": "OpenClaw",
}


def generate_agent_config(agent_key):
    """Generate agent-specific configuration."""
    display = AGENT_DISPLAY.get(agent_key, agent_key)

    # Detect workspace path
    workspace = os.getcwd()
    env_vars = {
        "qwen_code": "QWEN_CODE_WORKSPACE",
        "claude_code": "CLAUDE_CODE_WORKSPACE",
        "codex_cli": "CODEX_WORKSPACE",
        "opencode": "OPENCODE_WORKSPACE",
        "antigravity_ide": "ANTIGRAVITY_WORKSPACE",
        "kilo_cli": "KILO_WORKSPACE",
        "vscode": "VSCODE_CWD",
        "windsurf": "WINDSURF_WORKSPACE",
        "cursor": "CURSOR_WORKSPACE",
    }

    env_var = env_vars.get(agent_key)
    if env_var and os.environ.get(env_var):
        workspace = os.environ[env_var]

    return {
        "agent": display,
        "agent_key": agent_key,
        "workspace": workspace,
        "openmem_root": str(OPENMEM_ROOT),
        "data_dir": str(DATA_DIR),
        "memory": {
            "db_path": str(DATA_DIR / "lancedb"),
            "embedding_model": "all-MiniLM-L6-v2",
            "max_memories": 1000,
            "min_importance": 0.3,
            "auto_index": True,
        },
        "learning": {
            "auto_learn": True,
            "interval_hours": 2,
            "enable_skill_generation": True,
            "enable_reflection": True,
            "enable_consolidation": True,
        },
        "optimization": {
            "enabled": True,
            "prune_threshold": 0.2,
            "strengthen_threshold": 0.7,
        },
        "evolution": {
            "enabled": True,
            "population_size": 20,
            "mutation_rate": 0.15,
            "crossover_rate": 0.3,
        },
        "context_injection": {
            "enabled": True,
            "max_context_tokens": 4000,
            "format": "markdown",
        },
        "generated_at": datetime.now().isoformat(),
    }


def generate_master_config():
    """Generate the master config.json."""
    detected = os.environ.get("OPENMEM_AGENT", "auto-detect").lower()

    return {
        "version": "2.0.0",
        "agent": detected,
        "openmem_root": str(OPENMEM_ROOT),
        "data_dir": str(DATA_DIR),
        "llm": {
            "provider": "auto",
            "model": "auto",
            "api_key_env": "AUTO",
        },
        "memory": {
            "db_path": str(DATA_DIR / "lancedb"),
            "embedding_model": "all-MiniLM-L6-v2",
            "max_memories": 1000,
            "consolidation_schedule": "auto",
        },
        "learning": {
            "auto_learn": True,
            "interval_hours": 2,
            "enable_skill_generation": True,
            "enable_evolution": True,
        },
        "agents": {a: {"enabled": True} for a in SUPPORTED_AGENTS},
        "generated_at": datetime.now().isoformat(),
    }


def main():
    import argparse

    parser = argparse.ArgumentParser(description="OpenMem Config Generator")
    parser.add_argument("--agent", choices=SUPPORTED_AGENTS, help="Generate for specific agent")
    parser.add_argument("--all", action="store_true", help="Generate configs for all agents")
    parser.add_argument("--show", action="store_true", help="Show current master config")

    args = parser.parse_args()

    if args.show:
        if CONFIG_FILE.exists():
            with open(CONFIG_FILE, "r") as f:
                print(json.dumps(json.load(f), indent=2))
        else:
            print(json.dumps(generate_master_config(), indent=2))
        return

    if args.agent:
        config = generate_agent_config(args.agent)
        config_path = OPENMEM_ROOT / "agents" / args.agent / "config.json"
        with open(config_path, "w") as f:
            json.dump(config, f, indent=2)
        print(f"✅ Config for {AGENT_DISPLAY[args.agent]} → {config_path}")

    elif args.all:
        # Generate master config
        master = generate_master_config()
        with open(CONFIG_FILE, "w") as f:
            json.dump(master, f, indent=2)
        print(f"✅ Master config → {CONFIG_FILE}")

        # Generate per-agent configs
        for agent_key in SUPPORTED_AGENTS:
            config = generate_agent_config(agent_key)
            config_path = OPENMEM_ROOT / "agents" / agent_key / "config.json"
            with open(config_path, "w") as f:
                json.dump(config, f, indent=2)
            print(f"✅ {AGENT_DISPLAY[agent_key]} → {config_path}")

        print(f"\nDone! Generated configs for {len(SUPPORTED_AGENTS)} agents + master.")

    else:
        # Auto-detect and generate
        from bin.launcher import detect_agent
        detected = detect_agent()
        config = generate_agent_config(detected)

        # Write master config
        master = generate_master_config()
        master["agent"] = detected
        with open(CONFIG_FILE, "w") as f:
            json.dump(master, f, indent=2)
        print(f"✅ Detected: {AGENT_DISPLAY.get(detected, detected)}")
        print(f"✅ Master config → {CONFIG_FILE}")

        # Write agent-specific config
        agent_config_path = OPENMEM_ROOT / "agents" / detected / "config.json"
        with open(agent_config_path, "w") as f:
            json.dump(config, f, indent=2)
        print(f"✅ Agent config → {agent_config_path}")


if __name__ == "__main__":
    main()
