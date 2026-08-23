"""
OpenMem — Codex CLI Adapter (OpenAI).

Real session format (inventoried on this machine, see doc/session_formats.md):

- Storage: ~/.codex/sessions/YYYY/MM/DD/rollout-<ts>-<uuid>.jsonl
  (date-partitioned JSON Lines rollout files).
- Every record: {timestamp, ordinal, type, payload}.
- Conversation records: response_item with payload.type == "message" and
  payload.role of "user" / "assistant" / "developer". Content blocks are
  {"type": "input_text"|"output_text", "text": ...}.
- session_meta carries the canonical session id (payload.session_id).
- event_msg / turn_context / world_state are telemetry → skipped.

Skill install: ~/.codex/plugins/
Context injection: .codex/context.md
"""

import os
from pathlib import Path
from typing import Dict, List, Optional, Any, Callable
from datetime import datetime

from ..base import AgentAdapter, register_adapter


# Codex harness injects machine context as user-role messages wrapped in XML.
# Pure wrapper content is noise; real user prose never starts with these tags.
_CODEX_CONTEXT_PREFIXES = (
    "<environment_context>",
    "<recommended_plugins>",
    "<skills_instructions>",
    "<user_instructions>",
    "<permissions_hint>",
    "<project_instructions>",
)


class CodexCliAdapter(AgentAdapter):
    """OpenAI Codex CLI specific adapter with real rollout parsing."""

    AGENT_NAME = "Codex CLI"
    SKILL_FILES = ["SKILL.md", "learner.py"]

    def __init__(self, workspace: str = None, codex_dir: str = None):
        self._workspace = workspace or os.getcwd()
        self._codex_dir = codex_dir or os.path.join(
            os.path.expanduser("~"), ".codex"
        )
        self._session_dir = os.path.join(self._codex_dir, "sessions")
        self._message_hook = None

    # ------------------------------------------------------------------
    # Session discovery + parsing
    # ------------------------------------------------------------------

    def _discover_session_files(self, hours_back: int) -> List[str]:
        """Find rollout files across the date-partitioned tree, newest-first."""
        return self.find_session_files(
            self._session_dir, "**/rollout-*.jsonl", hours_back=hours_back
        )

    @staticmethod
    def _extract_block_text(blocks: Any) -> str:
        """Concatenate text from input_text/output_text content blocks."""
        if isinstance(blocks, str):
            return blocks.strip()
        parts = []
        if isinstance(blocks, list):
            for block in blocks:
                if not isinstance(block, dict):
                    continue
                if block.get("type") in ("input_text", "output_text"):
                    text = block.get("text", "")
                    if isinstance(text, str) and text.strip():
                        parts.append(text.strip())
        return "\n".join(parts).strip()

    def get_session_messages(self, limit: int = 100,
                             hours_back: int = 168) -> List[Dict[str, str]]:
        """
        Parse recent Codex rollout transcripts into normalized messages.

        Tolerant line-by-line parsing per doc/session_formats.md:
        malformed lines are skipped (and counted), telemetry record types are
        dropped, developer-role injections are dropped, and machine-injected
        XML-wrapped user messages are dropped.

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

            session_id = Path(fpath).stem
            for rec in records:
                if rec.get("type") == "session_meta":
                    meta_id = (rec.get("payload") or {}).get("session_id")
                    if meta_id:
                        session_id = meta_id
                    continue
                if rec.get("type") != "response_item":
                    continue

                msg = self._parse_response_item(rec, session_id)
                if msg is not None:
                    messages.append(msg)
                if len(messages) >= limit:
                    break
            if len(messages) >= limit:
                break

        if files_read or malformed_total:
            print(
                f"[Codex CLI] Parsed {len(messages)} messages from "
                f"{files_read} rollout file(s), skipped {malformed_total} malformed line(s)"
            )
        return messages[:limit]

    def _parse_response_item(self, rec: Dict[str, Any],
                             session_id: str) -> Optional[Dict[str, str]]:
        """Convert a response_item record to a message dict, or None."""
        payload = rec.get("payload") or {}
        if payload.get("type") != "message":
            return None

        role = payload.get("role")
        if role not in ("user", "assistant"):
            # "developer" holds injected instructions; anything else unknown.
            return None

        content = self._extract_block_text(payload.get("content"))
        if not content:
            return None

        if role == "user" and content.startswith(_CODEX_CONTEXT_PREFIXES):
            return None

        return {
            "role": role,
            "content": content,
            "timestamp": rec.get("timestamp", ""),
            "session_id": session_id,
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
        try:
            ctx_file = os.path.join(self._workspace, ".codex", "context.md")
            os.makedirs(os.path.dirname(ctx_file), exist_ok=True)
            with open(ctx_file, "w", encoding="utf-8") as f:
                f.write(f"# Memory Context\n\n{context}\n")
            return True
        except OSError:
            return False

    def get_workspace_path(self) -> str:
        return os.path.abspath(self._workspace)

    def get_session_id(self) -> str:
        files = self._discover_session_files(hours_back=24)
        if files:
            stem = Path(files[0]).stem
            # rollout-<timestamp>-<uuid> → keep the trailing uuid part
            return stem.split("-", 2)[-1] if stem.count("-") >= 2 else stem
        return f"codex_{datetime.now().strftime('%Y%m%d_%H%M%S')}"

    def get_agent_name(self) -> str:
        return self.AGENT_NAME

    def get_skill_install_path(self) -> Optional[str]:
        return os.path.join(self._codex_dir, "plugins", "openmem")

    def register_message_hook(self, callback: Callable[[Dict], None]) -> bool:
        self._message_hook = callback
        return True


register_adapter("codex_cli", CodexCliAdapter)
register_adapter("codex", CodexCliAdapter)
