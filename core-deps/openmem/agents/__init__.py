"""
OpenMem Agent Adapters — Agent-Agnostic Memory Layer.

Supports: Qwen Code, Claude Code, Codex CLI, OpenCode, Antigravity IDE,
Kilo CLI, VS Code, Windsurf, Cursor, OpenClaw, and any generic agent.
"""

from .base import (
    AgentAdapter,
    register_adapter,
    get_adapter,
    get_available_adapters,
    auto_detect_adapter,
    resolve_agent_adapter,
)

# Register all adapters on import
from . import generic
from . import qwen_code
from . import claude_code
from . import codex_cli
from . import opencode
from . import antigravity_ide
from . import kilo_cli
from . import vscode
from . import windsurf
from . import cursor
from . import openclaw

__all__ = [
    "AgentAdapter",
    "register_adapter",
    "get_adapter",
    "get_available_adapters",
    "auto_detect_adapter",
    "resolve_agent_adapter",
]
