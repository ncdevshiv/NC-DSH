"""
OpenMem — Generic Agent Adapter (Fallback).

Works with any agent that stores session data in a standard workspace directory.
This is the default when no specific adapter matches.
"""

import os
import json
from pathlib import Path
from typing import Dict, List, Optional, Any, Callable
from datetime import datetime

from ..base import AgentAdapter, register_adapter


class GenericAdapter(AgentAdapter):
    """
    Generic agent adapter — works with any agent using standard file-based sessions.

    Detects sessions from:
    - GENERIC_SESSION_DIR env var
    - <workspace>/.sessions/ directory
    - <workspace>/sessions/ directory
    """

    AGENT_NAME = "Generic"

    def __init__(self):
        self._workspace = None
        self._session_dir = None
        self._current_session_id = None
        self._context_buffer = ""
        self._message_hook = None
        self._detect_workspace()

    def _detect_workspace(self):
        """Auto-detect workspace and session directories."""
        # Check env var
        env_workspace = os.environ.get("GENERIC_WORKSPACE") or \
                        os.environ.get("GENERIC_SESSION_DIR")
        if env_workspace:
            self._workspace = env_workspace
            self._session_dir = env_workspace
            return

        # Check CWD
        cwd = os.getcwd()
        for candidate in [".sessions", "sessions", ".session", ".openmem"]:
            candidate_path = os.path.join(cwd, candidate)
            if os.path.isdir(candidate_path):
                self._workspace = cwd
                self._session_dir = candidate_path
                return

        # Use CWD as workspace, create sessions dir
        self._workspace = cwd
        self._session_dir = os.path.join(cwd, ".openmem", "sessions")
        os.makedirs(self._session_dir, exist_ok=True)

    def get_session_messages(self, limit: int = 100) -> List[Dict[str, str]]:
        """Read messages from session JSON files."""
        messages = []

        if not self._session_dir or not os.path.isdir(self._session_dir):
            return messages

        session_files = self.find_session_files(self._session_dir, "*.json", hours_back=168)

        for fpath in sorted(session_files, key=lambda f: os.path.getmtime(f), reverse=True):
            data = self.load_session_json(fpath)
            if not data:
                continue

            # Standard format: {"messages": [{"role": ..., "content": ...}]}
            msg_list = data.get("messages", [])
            if isinstance(msg_list, list):
                for msg in msg_list:
                    if isinstance(msg, dict) and "content" in msg:
                        messages.append({
                            "role": msg.get("role", "unknown"),
                            "content": msg.get("content", ""),
                            "timestamp": msg.get("timestamp", ""),
                        })

            # Flat list format
            elif isinstance(data, list):
                for msg in data:
                    if isinstance(msg, dict):
                        messages.append({
                            "role": msg.get("role", "unknown"),
                            "content": msg.get("content", ""),
                            "timestamp": msg.get("timestamp", ""),
                        })

            if len(messages) >= limit:
                break

        return messages[:limit]

    def inject_context(self, context: str) -> bool:
        """Store context in buffer for next session."""
        self._context_buffer = context

        # Also write to context file if session dir exists
        if self._session_dir:
            try:
                ctx_file = os.path.join(self._session_dir, "_memory_context.txt")
                with open(ctx_file, "w", encoding="utf-8") as f:
                    f.write(context)
                return True
            except OSError:
                pass
        return False

    def get_workspace_path(self) -> str:
        return os.path.abspath(self._workspace or os.getcwd())

    def get_session_id(self) -> str:
        if self._current_session_id:
            return self._current_session_id

        # Generate from most recent session file
        if self._session_dir:
            session_files = self.find_session_files(self._session_dir, "*.json", hours_back=24)
            if session_files:
                latest = max(session_files, key=lambda f: os.path.getmtime(f))
                self._current_session_id = Path(latest).stem
                return self._current_session_id

        # Generate new ID
        self._current_session_id = f"generic_{datetime.now().strftime('%Y%m%d_%H%M%S')}"
        return self._current_session_id

    def get_agent_name(self) -> str:
        return self.AGENT_NAME

    def get_skill_install_path(self) -> Optional[str]:
        workspace = self.get_workspace_path()
        return os.path.join(workspace, ".openmem", "skills", "generic")

    def register_message_hook(self, callback: Callable[[Dict], None]) -> bool:
        self._message_hook = callback
        return True

    def save_message(self, role: str, content: str, metadata: Dict = None):
        """
        Save a message to the session file (for agents without native storage).

        Args:
            role: "user" or "assistant"
            content: Message content
            metadata: Optional metadata dict
        """
        if not self._session_dir:
            return

        session_file = os.path.join(
            self._session_dir,
            f"{self.get_session_id()}.json"
        )

        # Load existing or create new
        data = self.load_session_json(session_file) or {"messages": []}

        msg = {
            "role": role,
            "content": content,
            "timestamp": datetime.now().isoformat(),
        }
        if metadata:
            msg["metadata"] = metadata

        data["messages"].append(msg)

        try:
            with open(session_file, "w", encoding="utf-8") as f:
                json.dump(data, f, indent=2)
        except OSError:
            pass

        # Trigger hook
        if self._message_hook:
            self._message_hook(msg)


# Register on module load
register_adapter("generic", GenericAdapter)
