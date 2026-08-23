"""
OpenMem — VS Code Adapter.

Session storage: VS Code extension storage (~/.vscode/extensions/openmem-*/sessions/)
Skill install: VS Code extension or workspace .vscode/
Context injection: .vscode/memory.md (referenced by Copilot/custom extension)
"""

import os
from pathlib import Path
from typing import Dict, List, Optional, Callable
from datetime import datetime

from ..base import AgentAdapter, register_adapter


class VscodeAdapter(AgentAdapter):
    """VS Code specific adapter."""

    AGENT_NAME = "VS Code"
    SKILL_FILES = ["SKILL.md", "learner.py"]

    def __init__(self):
        self._workspace = os.environ.get("VSCODE_CWD", os.getcwd())
        self._vscode_dir = os.path.join(os.path.expanduser("~"), ".vscode")
        self._session_dir = os.path.join(self._vscode_dir, "openmem-sessions")
        os.makedirs(self._session_dir, exist_ok=True)
        self._message_hook = None

    def get_session_messages(self, limit: int = 100) -> List[Dict[str, str]]:
        messages = []
        if not os.path.isdir(self._session_dir):
            return messages
        for fpath in sorted(
            self.find_session_files(self._session_dir, "*.json", hours_back=168),
            key=lambda f: os.path.getmtime(f), reverse=True
        ):
            data = self.load_session_json(fpath)
            if not data:
                continue
            for msg in data.get("messages", []):
                if isinstance(msg, dict) and "content" in msg:
                    messages.append({
                        "role": msg.get("role", "unknown"),
                        "content": msg["content"],
                        "timestamp": msg.get("timestamp", ""),
                    })
            if len(messages) >= limit:
                break
        return messages[:limit]

    def inject_context(self, context: str) -> bool:
        try:
            ctx_file = os.path.join(self._workspace, ".vscode", "memory.md")
            os.makedirs(os.path.dirname(ctx_file), exist_ok=True)
            with open(ctx_file, "w", encoding="utf-8") as f:
                f.write(f"# OpenMem Memory Context\n\n{context}\n")
            return True
        except OSError:
            return False

    def get_workspace_path(self) -> str:
        return os.path.abspath(self._workspace)

    def get_session_id(self) -> str:
        if os.path.isdir(self._session_dir):
            files = self.find_session_files(self._session_dir, "*.json", hours_back=24)
            if files:
                return Path(max(files, key=lambda f: os.path.getmtime(f))).stem
        return f"vscode_{datetime.now().strftime('%Y%m%d_%H%M%S')}"

    def get_agent_name(self) -> str:
        return self.AGENT_NAME

    def get_skill_install_path(self) -> Optional[str]:
        return os.path.join(self._workspace, ".vscode", "openmem")

    def register_message_hook(self, callback: Callable[[Dict], None]) -> bool:
        self._message_hook = callback
        return True


register_adapter("vscode", VscodeAdapter)
