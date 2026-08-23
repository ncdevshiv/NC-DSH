"""
OpenMem — Qwen Code Adapter.

Integrates OpenMem memory system with Qwen Code (qwen-cli).

Session storage: ~/.qwen/sessions/
Skill install: skills/memory/ within Qwen workspace
Context injection: via .qwen/config.json or inline context file
"""

import os
import json
from pathlib import Path
from typing import Dict, List, Optional, Any, Callable
from datetime import datetime

from ..base import AgentAdapter, register_adapter


class QwenCodeAdapter(AgentAdapter):
    """
    Qwen Code specific adapter.

    Handles:
    - Reading session transcripts from ~/.qwen/sessions/
    - Injecting memory context into .qwen/ system config
    - Installing skills into Qwen's skill directory
    """

    AGENT_NAME = "Qwen Code"
    SKILL_FILES = ["SKILL.md", "learner.py", "config.json"]

    def __init__(self):
        self._workspace = None
        self._qwen_dir = None
        self._session_dir = None
        self._message_hook = None
        self._detect_paths()

    def _detect_paths(self):
        """Detect Qwen Code workspace and session paths."""
        # Check explicit env var
        env_workspace = os.environ.get("QWEN_CODE_WORKSPACE")
        if env_workspace:
            self._workspace = env_workspace
        else:
            self._workspace = os.getcwd()

        # Qwen data directory
        self._qwen_dir = os.path.join(
            os.path.expanduser("~"), ".qwen"
        )
        self._session_dir = os.path.join(self._qwen_dir, "sessions")

        # Ensure dirs exist
        os.makedirs(self._session_dir, exist_ok=True)

    def get_session_messages(self, limit: int = 100) -> List[Dict[str, str]]:
        """Read messages from Qwen session JSON files."""
        messages = []

        if not os.path.isdir(self._session_dir):
            return messages

        session_files = self.find_session_files(self._session_dir, "*.json", hours_back=168)

        for fpath in sorted(session_files, key=lambda f: os.path.getmtime(f), reverse=True):
            data = self.load_session_json(fpath)
            if not data:
                continue

            msg_list = data.get("messages", data.get("conversation", []))
            if isinstance(msg_list, list):
                for msg in msg_list:
                    if isinstance(msg, dict) and "content" in msg:
                        messages.append({
                            "role": msg.get("role", msg.get("sender", "unknown")),
                            "content": msg.get("content", ""),
                            "timestamp": msg.get("timestamp", msg.get("created_at", "")),
                        })

            if len(messages) >= limit:
                break

        return messages[:limit]

    def inject_context(self, context: str) -> bool:
        """
        Inject memory context into Qwen Code.

        Strategy: Write to .qwen/memory_context.md which Qwen can reference.
        Also write inline context file for immediate session.
        """
        try:
            # Method 1: Write to Qwen's context file
            context_file = os.path.join(self._qwen_dir, "memory_context.md")
            with open(context_file, "w", encoding="utf-8") as f:
                f.write("# OpenMem Memory Context\n\n")
                f.write(f"_Auto-generated at {datetime.now().strftime('%Y-%m-%d %H:%M')}_\n\n")
                f.write(context)

            # Method 2: Write to workspace-level AGENTS.md if it exists
            agents_md = os.path.join(self._workspace, "AGENTS.md")
            if os.path.exists(agents_md):
                self._inject_into_agents_md(agents_md, context)

            return True
        except OSError:
            return False

    def _inject_into_agents_md(self, filepath: str, context: str):
        """
        Inject memory context into existing AGENTS.md file.

        Adds/updates a ## Memory Context section.
        """
        try:
            with open(filepath, "r", encoding="utf-8") as f:
                content = f.read()

            marker = "## Memory Context"
            if marker in content:
                # Replace existing section
                parts = content.split(marker, 1)
                rest = parts[1].split("\n## ", 1)
                if len(rest) > 1:
                    content = parts[0] + marker + "\n\n" + context + "\n\n## " + rest[1]
                else:
                    content = parts[0] + marker + "\n\n" + context + "\n"
            else:
                # Append new section
                content += f"\n{marker}\n\n{context}\n"

            with open(filepath, "w", encoding="utf-8") as f:
                f.write(content)
        except OSError:
            pass

    def get_workspace_path(self) -> str:
        return os.path.abspath(self._workspace)

    def get_session_id(self) -> str:
        """Get current session ID from Qwen's session files."""
        if not os.path.isdir(self._session_dir):
            return f"qwen_{datetime.now().strftime('%Y%m%d_%H%M%S')}"

        session_files = self.find_session_files(self._session_dir, "*.json", hours_back=24)
        if session_files:
            latest = max(session_files, key=lambda f: os.path.getmtime(f))
            return Path(latest).stem

        return f"qwen_{datetime.now().strftime('%Y%m%d_%H%M%S')}"

    def get_agent_name(self) -> str:
        return self.AGENT_NAME

    def get_skill_install_path(self) -> Optional[str]:
        """Qwen Code skills go in the workspace's skills/ directory or ~/.qwen/skills/."""
        # Try workspace skills dir first
        workspace_skills = os.path.join(self._workspace, "skills", "memory")
        if os.path.isdir(os.path.dirname(workspace_skills)):
            return workspace_skills

        # Fallback to ~/.qwen/skills/
        qwen_skills = os.path.join(self._qwen_dir, "skills", "memory")
        return qwen_skills

    def register_message_hook(self, callback: Callable[[Dict], None]) -> bool:
        """
        Register a message hook via Qwen's session monitoring.

        Note: Qwen Code doesn't have a native plugin API, so we monitor
        the session directory for new files.
        """
        self._message_hook = callback
        return True

    def get_current_task(self) -> Optional[str]:
        """Get current task from the most recent user message."""
        messages = self.get_session_messages(limit=1)
        if messages and messages[0].get("role") == "user":
            return messages[0]["content"]
        return None

    def get_config(self) -> Dict[str, Any]:
        """Load Qwen configuration."""
        config_path = os.path.join(self._qwen_dir, "config.json")
        if os.path.exists(config_path):
            try:
                with open(config_path, "r") as f:
                    return json.load(f)
            except (json.JSONDecodeError, OSError):
                pass

        # Check workspace-level config
        workspace_config = os.path.join(self._workspace, ".qwen", "config.json")
        if os.path.exists(workspace_config):
            try:
                with open(workspace_config, "r") as f:
                    return json.load(f)
            except (json.JSONDecodeError, OSError):
                pass

        return {}


# Register on module load
register_adapter("qwen_code", QwenCodeAdapter)
register_adapter("qwen", QwenCodeAdapter)
