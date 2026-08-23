"""
OpenMem — Cursor Editor Adapter.

Session storage reality on this machine (see doc/session_formats.md):
~/.cursor/ exists but currently contains ONLY an empty sessions/ directory —
no chat transcripts in any file format. Cursor's richer state lives in
workspace state.vscdb sqlite databases, which are out of scope (no new deps,
schema unverified). This adapter therefore performs tolerant discovery over:

- ~/.cursor/sessions/**/*.json / *.jsonl
- <workspace>/.cursor/sessions/*.json

and returns [] when nothing exists. If real Cursor history files appear,
inventory them first (per doc/session_formats.md) before extending parsing.

Skill install: .cursor/rules/
Context injection: .cursor/rules/memory.md (Cursor reads rules dir)
"""

import os
import json
from pathlib import Path
from typing import Dict, List, Optional, Any, Callable
from datetime import datetime

from ..base import AgentAdapter, register_adapter


class CursorAdapter(AgentAdapter):
    """Cursor editor specific adapter (tolerant; no-op without local data)."""

    AGENT_NAME = "Cursor"
    SKILL_FILES = ["SKILL.md", "learner.py"]

    def __init__(self, workspace: str = None, cursor_dir: str = None):
        self._workspace = workspace or os.environ.get("CURSOR_WORKSPACE", os.getcwd())
        self._cursor_dir = cursor_dir or os.path.join(
            os.path.expanduser("~"), ".cursor"
        )
        self._session_dir = os.path.join(self._cursor_dir, "sessions")
        self._message_hook = None

    @staticmethod
    def _extract_messages_from_file(fpath: str) -> List[Dict]:
        """
        Best-effort parse of a Cursor session file.

        Accepts either a single JSON document containing a "messages" /
        "conversation" array of {role, content, timestamp} objects, or a
        JSON Lines file with one such object per line. Unparsable content
        is skipped silently — this format is not yet observed in the wild.
        """
        try:
            with open(fpath, "r", encoding="utf-8") as f:
                text = f.read()
        except OSError:
            return []

        candidates: List[Dict] = []
        stripped = text.lstrip()
        if stripped.startswith("[") or stripped.startswith("{"):
            try:
                doc = json.loads(text)
                if isinstance(doc, list):
                    candidates = [m for m in doc if isinstance(m, dict)]
                elif isinstance(doc, dict):
                    msg_list = doc.get("messages", doc.get("conversation", []))
                    if isinstance(msg_list, list):
                        candidates = [m for m in msg_list if isinstance(m, dict)]
            except json.JSONDecodeError:
                pass

        if not candidates and "\n" in text:
            # Tolerant JSONL fallback: keep lines that parse as message dicts
            for line in text.splitlines():
                line = line.strip()
                if not line:
                    continue
                try:
                    obj = json.loads(line)
                except json.JSONDecodeError:
                    continue
                if isinstance(obj, dict) and ("content" in obj or "role" in obj):
                    candidates.append(obj)

        return candidates

    def _session_search_dirs(self) -> List[str]:
        # Deduplicate: workspace .cursor and the home cursor dir can coincide
        seen = set()
        dirs = []
        for d in (
            os.path.join(self._workspace, ".cursor", "sessions"),
            self._session_dir,
        ):
            key = os.path.abspath(d).lower()
            if key not in seen:
                seen.add(key)
                dirs.append(d)
        return dirs

    def get_session_messages(self, limit: int = 100,
                             hours_back: int = 168) -> List[Dict[str, str]]:
        messages = []
        seen_any_file = False
        for session_dir in self._session_search_dirs():
            for fpath in self.find_session_files(
                session_dir, "**/*.json*", hours_back=hours_back
            ):
                seen_any_file = True
                sid = Path(fpath).stem
                for msg in self._extract_messages_from_file(fpath):
                    content = msg.get("content", "")
                    if not isinstance(content, str) or not content.strip():
                        continue
                    messages.append({
                        "role": msg.get("role", msg.get("sender", "unknown")),
                        "content": content.strip(),
                        "timestamp": msg.get("timestamp", ""),
                        "session_id": msg.get("session_id", sid),
                    })
                if len(messages) >= limit:
                    break
            if len(messages) >= limit:
                break

        if seen_any_file:
            print(
                f"[Cursor] Parsed {len(messages)} messages from "
                f"~/.cursor/sessions and workspace .cursor/sessions"
            )
        return messages[:limit]

    def get_recent_sessions(self, hours_back: int = 168,
                            limit: int = 100) -> List[Dict[str, Any]]:
        """Group parsed messages into sessions for the learning loop."""
        grouped: Dict[str, Dict[str, Any]] = {}
        for msg in self.get_session_messages(limit=limit, hours_back=hours_back):
            sid = msg.get("session_id") or "unknown"
            if sid not in grouped:
                grouped[sid] = {"id": sid, "path": "", "messages": []}
            grouped[sid]["messages"].append(msg)
        return list(grouped.values())

    def inject_context(self, context: str) -> bool:
        """
        Inject into Cursor's rules directory.
        Cursor automatically reads .cursor/rules/ files for system context.
        """
        try:
            # Primary: .cursor/rules/memory.md
            rules_dir = os.path.join(self._workspace, ".cursor", "rules")
            os.makedirs(rules_dir, exist_ok=True)
            rules_file = os.path.join(rules_dir, "memory.md")
            with open(rules_file, "w", encoding="utf-8") as f:
                f.write(f"# OpenMem Memory Context\n\n{context}\n")
            return True
        except OSError:
            return False

    def get_workspace_path(self) -> str:
        return os.path.abspath(self._workspace)

    def get_session_id(self) -> str:
        for session_dir in self._session_search_dirs():
            files = self.find_session_files(session_dir, "**/*.json*", hours_back=24)
            if files:
                return Path(files[0]).stem
        return f"cursor_{datetime.now().strftime('%Y%m%d_%H%M%S')}"

    def get_agent_name(self) -> str:
        return self.AGENT_NAME

    def get_skill_install_path(self) -> Optional[str]:
        return os.path.join(self._workspace, ".cursor", "rules")

    def register_message_hook(self, callback: Callable[[Dict], None]) -> bool:
        self._message_hook = callback
        return True


register_adapter("cursor", CursorAdapter)
