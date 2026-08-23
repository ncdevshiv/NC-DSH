"""
OpenMem — OpenClaw Adapter.

Session storage: ~/.openclaw/sessions/ or workspace memory/
Skill install: ~/.openclaw/skills/lancemem/
Context injection: learner.py command interface
"""

import os
from pathlib import Path
from typing import Dict, List, Optional, Any, Callable
from datetime import datetime

from ..base import AgentAdapter, register_adapter


class OpenclawAdapter(AgentAdapter):
    """OpenClaw specific adapter."""

    AGENT_NAME = "OpenClaw"
    SKILL_FILES = ["SKILL.md", "learner.py", "manifest.json"]

    def __init__(self, workspace: str = None):
        self._workspace = workspace or os.getcwd()
        self._openclaw_dir = os.path.join(os.path.expanduser("~"), ".openclaw")
        # Session search dirs: workspace sessions first, then the legacy
        # ~/.openclaw/sessions location. An explicit workspace redirects the
        # workspace-side paths (used by tests and alternate installs).
        if workspace:
            self._session_dirs = [
                os.path.join(workspace, "sessions"),
                os.path.join(self._openclaw_dir, "sessions"),
            ]
            self._memory_dir = os.path.join(workspace, "memory")
        else:
            self._session_dirs = [
                os.path.join(self._openclaw_dir, "workspace", "sessions"),
                os.path.join(self._openclaw_dir, "sessions"),
            ]
            self._memory_dir = os.path.join(
                self._openclaw_dir, "workspace", "memory"
            )
        for d in self._session_dirs + [self._memory_dir]:
            os.makedirs(d, exist_ok=True)
        self._message_hook = None

    def get_session_messages(self, limit: int = 100) -> List[Dict[str, str]]:
        messages = []
        for session in self.get_recent_sessions(limit=limit):
            messages.extend(session["messages"])
            if len(messages) >= limit:
                break
        return messages[:limit]

    def get_recent_sessions(self, hours_back: int = 168,
                            limit: int = 100) -> List[Dict[str, Any]]:
        """
        Enumerate OpenClaw session files into session dicts.

        Returns dicts with "id", "path", "messages" (normalized message
        dicts incl. session_id) and "data" (raw parsed document, kept for
        backward compatibility with ConversationIndexer.parse_session_messages).
        """
        sessions = []
        for session_dir in self._session_dirs + [self._memory_dir]:
            if not os.path.isdir(session_dir):
                continue
            for fpath in self.find_session_files(
                session_dir, "*.json", hours_back=hours_back
            ):
                data = self.load_session_json(fpath)
                if not isinstance(data, dict):
                    continue
                msg_list = data.get("messages", data.get("conversation", []))
                parsed = []
                for msg in msg_list:
                    if not isinstance(msg, dict) or "content" not in msg:
                        continue
                    content = msg.get("content", "")
                    if isinstance(content, list):
                        content = " ".join(
                            c.get("text", "") if isinstance(c, dict) else str(c)
                            for c in content
                        )
                    elif not isinstance(content, str):
                        content = str(content)
                    content = content.strip()
                    if not content:
                        continue
                    parsed.append({
                        "role": msg.get("role", msg.get("sender", "unknown")),
                        "content": content,
                        "timestamp": msg.get("timestamp",
                                             msg.get("created_at", "")),
                        "session_id": data.get("id", Path(fpath).stem),
                    })
                sessions.append({
                    "id": data.get("id", Path(fpath).stem),
                    "path": fpath,
                    "data": data,
                    "messages": parsed,
                })
                if len(sessions) >= limit:
                    return sessions
        return sessions

    def inject_context(self, context: str) -> bool:
        try:
            ctx_file = os.path.join(self._openclaw_dir, "memory_context.md")
            with open(ctx_file, "w", encoding="utf-8") as f:
                f.write(f"# OpenMem Memory Context\n\n{context}\n")
            return True
        except OSError:
            return False

    def get_workspace_path(self) -> str:
        return os.path.abspath(self._workspace)

    def get_session_id(self) -> str:
        for session_dir in self._session_dirs + [self._memory_dir]:
            if os.path.isdir(session_dir):
                files = self.find_session_files(session_dir, "*.json", hours_back=24)
                if files:
                    return Path(max(files, key=lambda f: os.path.getmtime(f))).stem
        return f"openclaw_{datetime.now().strftime('%Y%m%d_%H%M%S')}"

    def get_agent_name(self) -> str:
        return self.AGENT_NAME

    def get_skill_install_path(self) -> Optional[str]:
        return os.path.join(self._openclaw_dir, "skills", "lancemem")

    def register_message_hook(self, callback: Callable[[Dict], None]) -> bool:
        self._message_hook = callback
        return True

    def get_config(self) -> Dict:
        """Load OpenClaw config if available."""
        config_path = os.path.join(self._openclaw_dir, "config.json")
        if os.path.exists(config_path):
            try:
                import json
                with open(config_path, "r") as f:
                    return json.load(f)
            except (json.JSONDecodeError, OSError):
                pass
        return {}


register_adapter("openclaw", OpenclawAdapter)
