"""
OpenClaw Integration Module for OpenMem.

This module provides utilities for integrating OpenMem with OpenClaw's gateway.

Provenance (packaging repair): this content previously lived at F:\\openmem\\setup.py,
where its presence made legacy builds execute OpenClaw integration code instead of
calling setuptools' setup(). It was moved here verbatim because a full repository
search found no references to it (`import setup`, `from setup import ...`, runpy/exec
of setup.py, and none of its public symbols are imported elsewhere).

Relocation adjustments: the original used ``os.path.dirname(__file__)`` as the repo
root (for skills/self-learning, main.py and command registration). From this new
location that is no longer the repo root, so paths are anchored via _REPO_ROOT.
"""

import os
import sys
import json
import subprocess
from pathlib import Path
from datetime import datetime
from typing import Dict, Optional, Any


# Repo root (F:\openmem) — two levels above skills/openclaw/
_REPO_ROOT = Path(__file__).resolve().parents[2]

# OpenClaw paths
OPENCLAW_CONFIG = os.path.join(os.path.expanduser("~"), ".openclaw", "openclaw.json")
OPENCLAW_SKILLS_DIR = os.path.join(
    os.path.expanduser("~"), 
    "AppData", "Roaming", "npm", "node_modules", "openclaw", "skills"
)
WORKSPACE_DIR = os.path.join(os.path.expanduser("~"), ".openclaw", "workspace")


def get_openclaw_config() -> Optional[Dict]:
    """Load OpenClaw configuration."""
    if os.path.exists(OPENCLAW_CONFIG):
        with open(OPENCLAW_CONFIG, 'r') as f:
            return json.load(f)
    return None


def check_openclaw_installed() -> bool:
    """Check if OpenClaw is installed."""
    try:
        result = subprocess.run(
            ["openclaw", "--version"],
            capture_output=True,
            text=True,
            timeout=5
        )
        return result.returncode == 0
    except (FileNotFoundError, subprocess.TimeoutExpired):
        return False


def install_openmem_skill(skill_dir: str = None) -> bool:
    """
    Install OpenMem skill to OpenClaw.
    
    Args:
        skill_dir: Path to self-learning skill directory
        
    Returns:
        True if successful
    """
    skill_source = skill_dir or os.path.join(_REPO_ROOT, "skills", "self-learning")
    
    if not os.path.exists(skill_source):
        print(f"[OpenMem] Skill source not found: {skill_source}")
        return False
    
    # Find skills directory
    skills_target = OPENCLAW_SKILLS_DIR
    
    # Try to find it
    possible_paths = [
        Path.home() / "AppData" / "Roaming" / "npm" / "node_modules" / "openclaw" / "skills",
        Path.home() / ".openclaw" / "skills",
        Path(sys.prefix) / "openclaw" / "skills",
    ]
    
    for path in possible_paths:
        if path.parent.exists():
            skills_target = str(path)
            break
    
    target_dir = os.path.join(skills_target, "self-learning")
    
    try:
        os.makedirs(target_dir, exist_ok=True)
        
        # Copy files
        import shutil
        for file in os.listdir(skill_source):
            src = os.path.join(skill_source, file)
            if os.path.isfile(src):
                shutil.copy2(src, os.path.join(target_dir, file))
                print(f"[OpenMem] Copied: {file}")
        
        print(f"[OpenMem] Skill installed to: {target_dir}")
        return True
    
    except Exception as e:
        print(f"[OpenMem] Installation failed: {e}")
        return False


def create_openclaw_cron() -> str:
    """
    Generate OpenClaw cron job configuration.
    
    Returns cron setup instructions.
    """
    base_dir = str(_REPO_ROOT)
    main_py = os.path.join(base_dir, "main.py")
    
    cron_config = {
        "name": "OpenMem Learning Cycle",
        "schedule": {
            "kind": "cron",
            "expr": "0 */2 * * *",  # Every 2 hours
            "tz": "UTC"
        },
        "payload": {
            "kind": "agentTurn",
            "message": f"python {main_py} run-cycle"
        },
        "delivery": {
            "mode": "announce"
        }
    }
    
    return json.dumps(cron_config, indent=2)


def register_openmem_commands() -> bool:
    """
    Register OpenMem as an OpenClaw command provider.
    
    This would be called during OpenClaw startup.
    """
    # Check if there's a commands config
    config = get_openclaw_config()
    if not config:
        return False
    
    # Add command configuration
    if "commands" not in config:
        config["commands"] = {}
    
    config["commands"]["openmem"] = {
        "path": str(_REPO_ROOT),
        "main": "main.py",
        "commands": ["run-cycle", "status", "patterns", "skills", "search", "stats", "profile"]
    }
    
    # Save updated config
    try:
        with open(OPENCLAW_CONFIG, 'w') as f:
            json.dump(config, f, indent=2)
        return True
    except Exception as e:
        print(f"[OpenMem] Failed to register commands: {e}")
        return False


def get_memory_context(query: str, max_results: int = 5) -> str:
    """
    Get relevant memory context for a query.
    
    This is the main function for OpenClaw to call to get
    memory context for injection into agent context.
    
    Args:
        query: The query to search memory for
        max_results: Maximum number of results
        
    Returns:
        Formatted string with relevant memories
    """
    try:
        from memory_store.memory_manager import MemoryManager
        
        manager = MemoryManager()
        context = manager.get_memory_context(query, hours=168)  # Last 7 days
        
        return context
    
    except Exception as e:
        return f"[Memory context unavailable: {e}]"


def get_user_profile_summary() -> str:
    """
    Get user profile summary for OpenClaw context.
    
    Returns:
        Human-readable user profile
    """
    try:
        from memory_store.user_model import UserModel
        
        model = UserModel()
        return model.get_profile_summary()
    
    except Exception as e:
        return f"[User profile unavailable: {e}]"


def inject_memory_into_context(context: Dict[str, Any], query: str = None) -> Dict[str, Any]:
    """
    Inject OpenMem data into OpenClaw context.
    
    This is the main integration point - it modifies the context
    dict that gets passed to the agent.
    
    Args:
        context: OpenClaw context dict
        query: Optional query to search memory for
        
    Returns:
        Updated context dict
    """
    try:
        from memory_store.user_model import UserModel
        from memory_store.memory_manager import MemoryManager
        
        model = UserModel()
        manager = MemoryManager()
        
        # Get user context
        user_context = model.get_context_for_new_interaction()
        
        # Add to context
        context["user_profile"] = user_context
        
        # Get memory context if query provided
        if query:
            memory_context = manager.get_memory_context(query)
            context["memory_context"] = memory_context
        
        # Add response style hints
        style = model.get_preferred_response_style()
        context["preferred_response_style"] = style
        
        return context
    
    except Exception as e:
        context["openmem_error"] = str(e)
        return context


# CLI for integration setup
if __name__ == "__main__":
    import argparse
    
    parser = argparse.ArgumentParser(description="OpenMem OpenClaw Integration")
    parser.add_argument("action", choices=["install", "cron", "register", "test"])
    
    args = parser.parse_args()
    
    if args.action == "install":
        success = install_openmem_skill()
        sys.exit(0 if success else 1)
    
    elif args.action == "cron":
        print(create_openclaw_cron())
    
    elif args.action == "register":
        success = register_openmem_commands()
        if success:
            print("Commands registered successfully")
        else:
            print("Failed to register commands")
        sys.exit(0 if success else 1)
    
    elif args.action == "test":
        print("Testing OpenMem integration...")
        print(f"OpenClaw config: {OPENCLAW_CONFIG} - {'Found' if os.path.exists(OPENCLAW_CONFIG) else 'Not found'}")
        print(f"Skills dir: {OPENCLAW_SKILLS_DIR} - {'Found' if os.path.exists(OPENCLAW_SKILLS_DIR) else 'Not found'}")
        
        # Test memory context
        context = get_memory_context("test")
        print(f"Memory context retrieval: {'OK' if context is not None else 'Failed'}")
        
        profile = get_user_profile_summary()
        print(f"User profile: {'OK' if profile else 'Failed'}")
