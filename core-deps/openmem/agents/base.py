"""
OpenMem Agent Adapter Base — Agent-Agnostic Interface Contract.

Every agent adapter (Qwen Code, Claude Code, Cursor, VS Code, etc.) must
subclass AgentAdapter and implement the abstract methods below.

This ensures OpenMem works identically regardless of which AI agent is driving.
"""

import os
import json
import shutil
from abc import ABC, abstractmethod
from pathlib import Path
from typing import Dict, List, Optional, Any, Callable
from datetime import datetime


class AgentAdapter(ABC):
    """
    Abstract base class for agent-specific adapters.

    Each subclass handles:
    - Reading session messages from that agent's storage format
    - Injecting memory context into that agent's prompt/system config
    - Detecting workspace paths and session IDs
    - Installing OpenMem skills into that agent's skill directory
    - Registering message hooks for automatic indexing

    Usage:
        adapter = QwenCodeAdapter()
        messages = adapter.get_session_messages()
        adapter.inject_context("User prefers concise responses")
        adapter.install_skill("/path/to/openmem")
    """

    # Override in subclass
    AGENT_NAME: str = "unknown"
    SKILL_FILES = ["SKILL.md", "learner.py"]  # Files to copy for skill install

    # ------------------------------------------------------------------
    # Required: Every agent must implement these
    # ------------------------------------------------------------------

    @abstractmethod
    def get_session_messages(self, limit: int = 100) -> List[Dict[str, str]]:
        """
        Retrieve recent messages from the current/active session.

        Returns:
            List of dicts: [{"role": "user"|"assistant", "content": "...", "timestamp": "..."}]
        """
        ...

    @abstractmethod
    def inject_context(self, context: str) -> bool:
        """
        Inject memory context into the agent's system prompt or config.

        Args:
            context: Formatted memory context string

        Returns:
            True if injection succeeded
        """
        ...

    @abstractmethod
    def get_workspace_path(self) -> str:
        """
        Return the agent's workspace/project directory.

        Returns:
            Absolute path string
        """
        ...

    @abstractmethod
    def get_session_id(self) -> str:
        """
        Return the current session identifier.

        Returns:
            Session ID string
        """
        ...

    @abstractmethod
    def get_agent_name(self) -> str:
        """
        Return the human-readable agent name.

        Returns:
            e.g. "Qwen Code", "Claude Code", "Cursor"
        """
        ...

    @abstractmethod
    def get_skill_install_path(self) -> Optional[str]:
        """
        Return the path where OpenMem skills should be installed.

        Returns:
            Absolute path string, or None if not determinable
        """
        ...

    # ------------------------------------------------------------------
    # Optional: Override if agent supports these features
    # ------------------------------------------------------------------

    def register_message_hook(self, callback: Callable[[Dict], None]) -> bool:
        """
        Register a callback to be invoked on every new message.

        Args:
            callback: Function that receives message dicts

        Returns:
            True if hook was registered successfully
        """
        return False  # Default: not supported

    def get_current_task(self) -> Optional[str]:
        """
        Return the current task/prompt the agent is working on.

        Returns:
            Task string or None
        """
        return None

    def mark_task_complete(self) -> bool:
        """
        Mark the current task as complete (for indexing triggers).

        Returns:
            True if marked successfully
        """
        return False

    def get_config(self) -> Dict[str, Any]:
        """
        Return agent-specific configuration dict.

        Returns:
            Config dict (paths, settings, etc.)
        """
        return {}

    # ------------------------------------------------------------------
    # Provided: Session discovery for the learning loop
    # ------------------------------------------------------------------

    def get_recent_sessions(self, hours_back: int = 168,
                            limit: int = 100) -> List[Dict[str, Any]]:
        """
        Return recent sessions as dicts consumable by the ConversationIndexer.

        Default implementation calls get_session_messages() and groups the
        returned per-message dicts (which carry "session_id") into sessions.
        Adapters with direct file access may override for richer metadata.

        Args:
            hours_back: Only include files/messages from the last N hours
            limit: Maximum number of messages to consider

        Returns:
            List of session dicts:
            [{"id": str, "path": str, "messages": [msg, ...]}, ...]
        """
        grouped: Dict[str, Dict[str, Any]] = {}
        for msg in self.get_session_messages(limit=limit):
            sid = msg.get("session_id") or "unknown"
            if sid not in grouped:
                grouped[sid] = {"id": sid, "path": "", "messages": []}
            grouped[sid]["messages"].append(msg)

        return list(grouped.values())

    # ------------------------------------------------------------------
    # Provided: Skill installation (works for all agents)
    # ------------------------------------------------------------------

    def install_skill(self, openmem_root: str = None) -> Optional[str]:
        """
        Install OpenMem skills into this agent's skill directory.

        Args:
            openmem_root: Path to OpenMem installation (auto-detected if None)

        Returns:
            Path where skills were installed, or None on failure
        """
        install_path = self.get_skill_install_path()
        if not install_path:
            print(f"[{self.AGENT_NAME}] Cannot determine skill install path")
            return None

        if openmem_root is None:
            openmem_root = str(Path(__file__).parent.parent)

        agent_skill_dir = os.path.join(openmem_root, "agents", self.AGENT_NAME.lower().replace(" ", "_"), "skill")
        if not os.path.exists(agent_skill_dir):
            print(f"[{self.AGENT_NAME}] No skill found at {agent_skill_dir}")
            return None

        try:
            os.makedirs(install_path, exist_ok=True)
            for fname in self.SKILL_FILES:
                src = os.path.join(agent_skill_dir, fname)
                dst = os.path.join(install_path, fname)
                if os.path.exists(src):
                    shutil.copy2(src, dst)

            print(f"[{self.AGENT_NAME}] Skills installed to {install_path}")
            return install_path
        except Exception as e:
            print(f"[{self.AGENT_NAME}] Skill install failed: {e}")
            return None

    # ------------------------------------------------------------------
    # Provided: Context formatting helpers
    # ------------------------------------------------------------------

    def format_memory_context(self, memories: List[Dict]) -> str:
        """
        Format a list of memory results into a context string.

        Args:
            memories: List of memory dicts from vector DB search

        Returns:
            Formatted context string for injection
        """
        if not memories:
            return ""

        lines = ["## Relevant Memory Context\n"]
        for i, mem in enumerate(memories, 1):
            content = mem.get("content", "")[:300]
            tier = mem.get("metadata", {}).get("tier", "memory")
            importance = mem.get("importance", 0)
            lines.append(f"### {i}. [{tier}] (importance: {importance:.2f})")
            lines.append(content)
            lines.append("")

        return "\n".join(lines)

    def format_user_profile_context(self, profile: Dict) -> str:
        """
        Format user profile data into context string.

        Args:
            profile: User profile dict

        Returns:
            Formatted context string
        """
        if not profile:
            return ""

        lines = ["## User Profile\n"]
        for key, data in profile.items():
            if isinstance(data, dict):
                value = data.get("value", str(data))
                confidence = data.get("confidence", 0)
                lines.append(f"- **{key}**: {value} ({confidence:.0%})")
            else:
                lines.append(f"- **{key}**: {data}")

        return "\n".join(lines)

    # ------------------------------------------------------------------
    # Provided: Status reporting
    # ------------------------------------------------------------------

    def get_status(self) -> Dict[str, Any]:
        """
        Return agent adapter status.

        Returns:
            Status dict
        """
        return {
            "agent": self.get_agent_name(),
            "workspace": self.get_workspace_path(),
            "session_id": self.get_session_id(),
            "skill_install_path": self.get_skill_install_path(),
            "message_hook_supported": self.register_message_hook(lambda m: None),
        }

    # ------------------------------------------------------------------
    # Provided: Session file discovery (common pattern)
    # ------------------------------------------------------------------

    @staticmethod
    def find_session_files(directory: str, pattern: str = "*.json",
                           hours_back: int = 24) -> List[str]:
        """
        Find session files in a directory modified within N hours.

        Args:
            directory: Directory to search
            pattern: Glob pattern ("**/*.jsonl" recurses subdirectories)
            hours_back: Only files modified within this many hours

        Returns:
            List of file paths, newest-first by modification time
        """
        import time
        from pathlib import Path as _Path

        if not os.path.isdir(directory):
            return []

        cutoff = time.time() - (hours_back * 3600)
        files = []

        for fp in _Path(directory).glob(pattern):
            try:
                if fp.is_file() and fp.stat().st_mtime >= cutoff:
                    files.append(str(fp))
            except OSError:
                continue

        files.sort(key=lambda f: os.path.getmtime(f), reverse=True)
        return files

    # ------------------------------------------------------------------
    # Provided: Tolerant JSON Lines reading (shared by real parsers)
    # ------------------------------------------------------------------

    @staticmethod
    def read_jsonl_records(filepath: str) -> Any:
        """
        Read a JSON Lines file line-by-line, tolerating malformed lines.

        A line that fails json.loads is skipped and counted; remaining lines
        are still processed. Blank lines are ignored silently.

        Args:
            filepath: Path to the .jsonl file

        Returns:
            Tuple (records, malformed_count) where records is a List[Dict]
            of successfully parsed objects.
        """
        records: List[Dict] = []
        malformed = 0
        try:
            with open(filepath, "r", encoding="utf-8") as f:
                for raw_line in f:
                    line = raw_line.strip()
                    if not line:
                        continue
                    try:
                        obj = json.loads(line)
                    except json.JSONDecodeError:
                        malformed += 1
                        continue
                    if isinstance(obj, dict):
                        records.append(obj)
        except (OSError, UnicodeDecodeError) as e:
            print(f"[OpenMem] Cannot read session file {filepath}: {e}")
        return records, malformed

    @staticmethod
    def load_session_json(filepath: str) -> Optional[Dict]:
        """
        Load a JSON session file safely.

        Args:
            filepath: Path to JSON file

        Returns:
            Parsed dict or None on failure
        """
        try:
            with open(filepath, "r", encoding="utf-8") as f:
                return json.load(f)
        except (json.JSONDecodeError, OSError, UnicodeDecodeError):
            return None


# ------------------------------------------------------------------
# Registry: Auto-detect which adapter to use
# ------------------------------------------------------------------

_REGISTRY: Dict[str, type] = {}


def register_adapter(name: str, adapter_cls: type):
    """Register an adapter class under a canonical name."""
    _REGISTRY[name.lower()] = adapter_cls


def get_adapter(name: str) -> Optional[AgentAdapter]:
    """
    Get an adapter instance by name.

    Args:
        name: Agent name (e.g. "qwen_code", "claude_code", "cursor")

    Returns:
        AgentAdapter instance or None
    """
    cls = _REGISTRY.get(name.lower())
    if cls is None:
        return None
    return cls()


def get_available_adapters() -> List[str]:
    """Return list of registered adapter names."""
    return list(_REGISTRY.keys())


def auto_detect_adapter() -> AgentAdapter:
    """
    Auto-detect the best adapter based on environment clues.

    Checks (in order):
    - OPENMEM_AGENT env var
    - Agent-specific env vars
    - On-disk evidence: first registered adapter whose discovery actually
      returns real session messages from this machine's history stores
    - Falls back to the generic adapter.
    """
    # Check explicit env var first
    agent_env = os.environ.get("OPENMEM_AGENT")
    if agent_env:
        adapter = get_adapter(agent_env)
        if adapter:
            return adapter

    # Check agent-specific environment variables
    agent_checks = [
        ("qwen_code", "QWEN_CODE_WORKSPACE"),
        ("claude_code", "CLAUDE_CODE_WORKSPACE"),
        ("cursor", "CURSOR_WORKSPACE"),
        ("vscode", "VSCODE_CWD"),
        ("codex_cli", "CODEX_WORKSPACE"),
        ("opencode", "OPENCODE_WORKSPACE"),
        ("windsurf", "WINDSURF_WORKSPACE"),
    ]

    for agent_name, env_var in agent_checks:
        if os.environ.get(env_var):
            adapter = get_adapter(agent_name)
            if adapter:
                return adapter

    # Probe on-disk history stores: prefer the adapter that can actually
    # read recent conversation history on this machine. Probing is cheap:
    # each adapter only reads its newest session file(s) with limit=1.
    probe_order = [
        "claude_code", "codex_cli", "openclaw", "cursor", "qwen_code",
        "opencode", "windsurf", "vscode", "antigravity_ide", "kilo_cli",
    ]
    for agent_name in probe_order:
        adapter = get_adapter(agent_name)
        if adapter is None:
            continue
        try:
            sessions = adapter.get_recent_sessions(hours_back=24 * 30, limit=1)
        except Exception:
            continue
        if sessions:
            print(f"[OpenMem] Auto-detected agent '{agent_name}' "
                  f"from on-disk session history")
            return adapter

    # Fallback: return generic adapter
    from agents.generic.adapter import GenericAdapter
    return GenericAdapter()


def resolve_agent_adapter(preferred: Optional[str] = None) -> AgentAdapter:
    """
    Resolve which agent adapter the learning loop should use.

    Resolution order:
    1. Explicit `preferred` argument (e.g. from code or CLI flag)
    2. OPENMEM_AGENT environment variable
    3. config.json "agent" field, when it names a concrete adapter
       (the value "auto-detect" is ignored and detection proceeds)
    4. auto_detect_adapter() — env clues, then on-disk history evidence

    Args:
        preferred: Optional explicit adapter name

    Returns:
        An AgentAdapter instance (never None; generic as last resort)
    """
    if preferred:
        adapter = get_adapter(preferred)
        if adapter:
            return adapter
        print(f"[OpenMem] Unknown agent '{preferred}', falling back to detection")

    env_agent = os.environ.get("OPENMEM_AGENT")
    if env_agent:
        adapter = get_adapter(env_agent)
        if adapter:
            return adapter

    try:
        config_path = os.path.join(
            os.path.dirname(os.path.abspath(__file__)), "..", "config.json"
        )
        if os.path.exists(config_path):
            with open(config_path, "r", encoding="utf-8") as f:
                config_agent = json.load(f).get("agent")
            if config_agent and config_agent != "auto-detect":
                adapter = get_adapter(config_agent)
                if adapter:
                    return adapter
    except (json.JSONDecodeError, OSError):
        pass

    return auto_detect_adapter()
