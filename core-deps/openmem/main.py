#!/usr/bin/env python3
"""
OpenMem — Autonomous Memory System for AI Agents.
Agent-agnostic: works with Qwen Code, Claude Code, Cursor, VS Code,
OpenClaw, Windsurf, Codex CLI, OpenCode, Antigravity IDE, Kilo CLI, and more.

Unified entry point — delegates to bin/launcher.py for all operations.

Usage:
    python main.py                     # Status
    python main.py install             # Full installation
    python main.py run-cycle           # Run learning cycle
    python main.py search "query"      # Search memories
    python main.py daemon start        # Start daemon
    python main.py status              # System status
    python main.py --help              # All commands
"""

import os
import sys
from pathlib import Path

# Portable path setup
BASE_DIR = Path(__file__).parent
sys.path.insert(0, str(BASE_DIR))


def main():
    """Delegate to bin/launcher.py for all operations."""
    launcher_script = BASE_DIR / "bin" / "launcher.py"

    if launcher_script.exists():
        # Run launcher with same args
        sys.argv[0] = str(launcher_script)

        import runpy
        try:
            runpy.run_path(str(launcher_script), run_name="__main__")
            return 0
        except SystemExit as e:
            return e.code if e.code is not None else 0
        except Exception as e:
            print(f"[OpenMem] Launcher error: {e}")
            import traceback
            traceback.print_exc()
            return 1
    else:
        print("[OpenMem] bin/launcher.py not found")
        print("Run: python bin/install.py --all")
        return 1


if __name__ == "__main__":
    sys.exit(main())
