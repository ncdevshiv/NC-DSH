"""
OpenMem — Claude Code Adapter (Anthropic Claude CLI).

Real session format (inventoried on this machine, see doc/session_formats.md):

- Storage: ~/.claude/projects/<munged-cwd>/<session-uuid>.jsonl
  (JSON Lines; one typed record per line). Legacy ~/.claude/sessions/*.json
  is probed as a fallback for older layouts.
- Conversation records: type "user" and "assistant" carry message.role and
  message.content (plain string OR array of typed blocks).
- Everything else (attachment, queue-operation, last-prompt, atis-latch,
  custom-title, mode, system) is metadata/noise.

Skill install: ~/.claude/skills/
Context injection: CLAUDE.md system prompt file
"""

import os
import json
from pathlib import Path
from typing import Dict, List, Optional, Any, Callable
from datetime import datetime

from ..base import AgentAdapter, register_adapter


# User records that are CLI command echoes / injected caveats, not conversation.
_CLAUDE_NOISE_PREFIXES = (
    "<command-name>",
    "<command-message>",
    "<command-args>",
    "<local-command-caveat>",
    "<local-command-stdout>",
    "<system-reminder>",
)


def _extract_text_from_content(content: Any) -> str:
    """
    Extract conversation text from a Claude message.content value.

    Handles both observed layouts: a plain string, or an array of typed
    blocks where text lives in {"type": "text", "text": ...} entries.
    Blocks of any other type (tool_use, tool_result, thinking, images)
    are ignored — they are not user/assistant prose.
    """
    if isinstance(content, str):
        return content.strip()
    if isinstance(content, list):
        parts = []
        for block in content:
            if isinstance(block, dict) and block.get("type") == "text":
                text = block.get("text", "")
                if isinstance(text, str) and text.strip():
                    parts.append(text.strip())
        return "\n".join(parts).strip()
    if content is None:
        return ""
    return str(content).strip()


class ClaudeCodeAdapter(AgentAdapter):
    """Claude Code specific adapter with real JSONL session parsing."""

    AGENT_NAME = "Claude Code"
    SKILL_FILES = ["SKILL.md", "learner.py"]

    def __init__(self, workspace: str = None, claude_dir: str = None):
        self._workspace = workspace or os.getcwd()
        self._claude_dir = claude_dir or os.path.join(
            os.path.expanduser("~"), ".claude"
        )
        self._projects_dir = os.path.join(self._claude_dir, "projects")
        self._session_dir = os.path.join(self._claude_dir, "sessions")
        self._message_hook = None

    # ------------------------------------------------------------------
    # Session discovery + parsing
    # ------------------------------------------------------------------

    def _discover_session_files(self, hours_back: int) -> List[str]:
        """Find Claude Code transcript files, newest-first."""
        files: List[str] = []
        # Real layout: projects/<munged-cwd>/<uuid>.jsonl (recursive)
        files.extend(
            self.find_session_files(
                self._projects_dir, "**/*.jsonl", hours_back=hours_back
            )
        )
        # Legacy fallback probed but not required to exist
        files.extend(
            self.find_session_files(
                self._session_dir, "*.json", hours_back=hours_back
            )
        )
        return files

    def get_session_messages(self, limit: int = 100,
                             hours_back: int = 168) -> List[Dict[str, str]]:
        """
        Parse recent Claude Code transcripts into normalized messages.

        Tolerant line-by-line parsing per doc/session_formats.md:
        malformed JSON lines are skipped (and counted), noise record types
        are dropped, <synthetic> API-error assistant turns are dropped,
        CLI command echoes are dropped, sidechain records are excluded.

        Args:
            limit: Maximum number of messages to return
            hours_back: Only read files modified within this many hours

        Returns:
            List of dicts: [{"role", "content", "timestamp", "session_id"}]
        """
        messages: List[Dict[str, str]] = []
        files_read = 0
        malformed_total = 0

        for fpath in self._discover_session_files(hours_back=hours_back):
            records, malformed = self.read_jsonl_records(fpath)
            malformed_total += malformed
            if not records:
                continue
            files_read += 1

            for rec in records:
                msg = self._parse_record(rec, fallback_session_id=Path(fpath).stem)
                if msg is not None:
                    messages.append(msg)
                if len(messages) >= limit:
                    break
            if len(messages) >= limit:
                break

        if files_read or malformed_total:
            print(
                f"[Claude Code] Parsed {len(messages)} messages from "
                f"{files_read} session file(s), skipped {malformed_total} malformed line(s)"
            )
        return messages[:limit]

    def _parse_record(self, rec: Dict[str, Any],
                      fallback_session_id: str) -> Optional[Dict[str, str]]:
        """
        Convert one Claude Code JSONL record into a message dict, or None
        when the record is noise / empty / non-conversational.
        """
        rec_type = rec.get("type")
        if rec_type not in ("user", "assistant"):
            return None

        # Sidechain records are interleaved subagent transcripts, not the
        # main conversation. Excluded by default (see doc/session_formats.md).
        if rec.get("isSidechain"):
            return None

        message = rec.get("message")
        if not isinstance(message, dict):
            return None

        # Drop synthetic assistant turns: model="<synthetic>" records carry
        # provider error text (observed: auth failures), not real replies.
        if rec_type == "assistant" and message.get("model") == "<synthetic>":
            return None

        content = _extract_text_from_content(message.get("content"))
        if not content:
            return None

        if rec_type == "user" and content.startswith(_CLAUDE_NOISE_PREFIXES):
            return None

        return {
            "role": "user" if rec_type == "user" else "assistant",
            "content": content,
            "timestamp": rec.get("timestamp", ""),
            "session_id": rec.get("sessionId") or fallback_session_id,
        }

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
        """Inject via CLAUDE.md in workspace root."""
        try:
            claude_md = os.path.join(self._workspace, "CLAUDE.md")
            marker = "## Memory Context"
            if os.path.exists(claude_md):
                with open(claude_md, "r", encoding="utf-8") as f:
                    content = f.read()
                if marker in content:
                    parts = content.split(marker, 1)
                    rest = parts[1].split("\n## ", 1)
                    content = parts[0] + marker + "\n\n" + context + (
                        "\n\n## " + rest[1] if len(rest) > 1 else "\n"
                    )
                else:
                    content += f"\n{marker}\n\n{context}\n"
                with open(claude_md, "w", encoding="utf-8") as f:
                    f.write(content)
            else:
                with open(claude_md, "w", encoding="utf-8") as f:
                    f.write(f"# Claude Code Project Context\n\n{marker}\n\n{context}\n")
            return True
        except OSError:
            return False

    def get_workspace_path(self) -> str:
        return os.path.abspath(self._workspace)

    def get_session_id(self) -> str:
        files = self._discover_session_files(hours_back=24)
        if files:
            return Path(files[0]).stem
        return f"claude_{datetime.now().strftime('%Y%m%d_%H%M%S')}"

    def get_agent_name(self) -> str:
        return self.AGENT_NAME

    def get_skill_install_path(self) -> Optional[str]:
        return os.path.join(self._claude_dir, "skills", "memory")

    def register_message_hook(self, callback: Callable[[Dict], None]) -> bool:
        self._message_hook = callback
        return True


register_adapter("claude_code", ClaudeCodeAdapter)
register_adapter("claude", ClaudeCodeAdapter)
