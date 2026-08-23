"""
Conversation Indexer for OpenMem.
Indexes agent session transcripts into the vector database.

Adapter-driven since Phase 1: session discovery and parsing live in
agents/<name>/adapter.py (real formats inventoried in doc/session_formats.md).
The resolved adapter can be forced via OPENMEM_AGENT env var or config.json;
otherwise on-disk history evidence decides. The OpenClaw branch is preserved
for backward compatibility.
"""

import os
import json
import sqlite3
import hashlib
from datetime import datetime, timedelta
from typing import Dict, List, Any, Optional
from pathlib import Path

from memory_store import get_vector_db

# Upper bound on persisted content hashes in index_state.json. Oldest
# entries are dropped first (list order == insertion order); large enough
# that real history never wraps during normal operation.
MAX_PERSISTED_CONTENT_HASHES = 50000


class ConversationIndexer:
    """
    Indexes agent conversations into the vector database.

    Handles:
    - Discovering recent sessions through the resolved AgentAdapter
    - Parsing and segmenting conversations (per-format parsers in adapters)
    - Extracting metadata (timestamps, role, session id)
    - Storing in vector DB with importance scoring
    - Session-id based dedup across runs (idempotent indexing)
    """

    def __init__(self, openclaw_workspace: str = None,
                 adapter: Any = None, agent_name: str = None,
                 vector_db: Any = None, index_state_file: str = None):
        # Dependency-injectable vector store so tests can redirect storage;
        # production always uses the shared singleton. NOTE: explicit None
        # check — LanceDBVectorStore defines __len__ (row count), so an empty
        # store is falsy and must not be replaced by `or`.
        self.vector_db = vector_db if vector_db is not None else get_vector_db()

        # Adapter resolution:
        # 1. Explicit adapter instance (tests / programmatic use)
        # 2. Explicit openclaw_workspace → OpenClaw branch (legacy behavior)
        # 3. resolve_agent_adapter(): explicit name > OPENMEM_AGENT env >
        #    config.json > auto-detect from on-disk history evidence
        self.adapter = adapter
        if self.adapter is None and openclaw_workspace:
            from agents.openclaw.adapter import OpenclawAdapter
            self.adapter = OpenclawAdapter(workspace=openclaw_workspace)
        if self.adapter is None:
            try:
                from agents.base import resolve_agent_adapter
                self.adapter = resolve_agent_adapter(preferred=agent_name)
            except Exception as e:
                print(f"[Indexer] Adapter resolution failed ({e}); "
                      f"falling back to generic adapter")
                from agents.base import get_adapter
                self.adapter = get_adapter("generic")

        # Legacy OpenClaw workspace paths kept for backward compatibility of
        # attributes that other code/tests may inspect.
        self.workspace = openclaw_workspace or os.path.join(
            os.path.expanduser("~"), ".openclaw", "workspace"
        )
        self.sessions_dir = os.path.join(self.workspace, "sessions")
        self.memory_dir = os.path.join(self.workspace, "memory")

        # Messages of sessions freshly indexed by the most recent run_indexing,
        # keyed by session id. The scheduler feeds these to the reflection
        # engine so reflections reference real conversation content.
        self.last_new_session_messages: Dict[str, List[Dict]] = {}

        # Duplicate messages skipped by content-hash dedup during the most
        # recent run_indexing (surfaced as report["duplicates_skipped"]).
        self.last_duplicates_skipped = 0

        # Index tracking
        self.index_state_file = index_state_file or os.path.join(
            os.path.dirname(__file__), "..", "data", "sessions", "index_state.json"
        )
        os.makedirs(os.path.dirname(os.path.abspath(self.index_state_file)), exist_ok=True)
        self.index_state = self._load_index_state()
    
    def _load_index_state(self) -> Dict:
        """Load the indexing state (last indexed sessions)."""
        if os.path.exists(self.index_state_file):
            with open(self.index_state_file, 'r') as f:
                return self._migrate_index_state(json.load(f))
        return self._fresh_index_state()

    @staticmethod
    def _fresh_index_state() -> Dict:
        """Fresh state layout including the content-hash dedup fields."""
        return {
            "indexed_sessions": [],  # List of session IDs already indexed
            "last_index_run": None,
            "total_messages_indexed": 0,
            # Cross-session content dedup (sha256 of normalized content).
            # Backfill is FORWARD-ONLY: the set starts empty on first run
            # after the upgrade and only grows from newly indexed messages;
            # already-indexed history is never retroactively scanned.
            "content_hashes": [],
            "content_hash_dedup_since": datetime.now().isoformat(),
        }

    @staticmethod
    def _migrate_index_state(state: Dict) -> Dict:
        """Upgrade an older state file in place to the dedup-aware layout."""
        if "content_hashes" not in state:
            state["content_hashes"] = []
            state["content_hash_dedup_since"] = datetime.now().isoformat()
        return state

    def _save_index_state(self):
        """Save indexing state."""
        self.index_state["last_index_run"] = datetime.now().isoformat()
        hashes = self.index_state.get("content_hashes")
        if isinstance(hashes, list) and len(hashes) > MAX_PERSISTED_CONTENT_HASHES:
            self.index_state["content_hashes"] = \
                hashes[-MAX_PERSISTED_CONTENT_HASHES:]
        with open(self.index_state_file, 'w') as f:
            json.dump(self.index_state, f, indent=2)

    @staticmethod
    def _content_hash(content: str) -> str:
        """
        Normalized content hash used for cross-session dedup.

        sha256 over content.strip().lower(): identical text posted in any
        two sessions collapses to one memory regardless of case or
        surrounding whitespace.
        """
        return hashlib.sha256(content.strip().lower().encode("utf-8")).hexdigest()
    
    def _agent_key(self) -> str:
        """Composite key prefix so ids from different agents never collide."""
        try:
            return self.adapter.AGENT_NAME.lower().replace(" ", "_")
        except AttributeError:
            return "unknown_agent"

    def get_sessions(self, hours_back: int = 24) -> List[Dict]:
        """
        Get recent sessions via the resolved agent adapter.

        Returns list of session dicts shaped by the adapter:
        {"id", "path", "messages": [...], ...}. Messages are already parsed
        into {"role", "content", "timestamp", "session_id"} dicts.
        """
        sessions = []
        try:
            sessions = self.adapter.get_recent_sessions(
                hours_back=hours_back, limit=100
            )
        except Exception as e:
            print(f"[Indexer] Adapter session discovery failed: {e}")
        return sessions

    def get_openclaw_sessions(self, hours_back: int = 24) -> List[Dict]:
        """
        Legacy OpenClaw session discovery (kept for backward compatibility).

        Reads session JSON files from the OpenClaw workspace sessions dir and
        ~/.openclaw/sessions. New code should use get_sessions(), which is
        adapter-driven.
        """
        sessions = []

        # Try to read from sessions directory
        if os.path.exists(self.sessions_dir):
            for session_file in Path(self.sessions_dir).glob("*.json"):
                try:
                    with open(session_file, 'r', encoding='utf-8') as f:
                        session_data = json.load(f)

                    # Get last modified time
                    mtime = datetime.fromtimestamp(os.path.getmtime(session_file))

                    # Filter by hours
                    if (datetime.now() - mtime).total_seconds() / 3600 <= hours_back:
                        sessions.append({
                            "id": session_file.stem,
                            "path": str(session_file),
                            "last_modified": mtime.isoformat(),
                            "data": session_data
                        })
                except Exception as e:
                    print(f"[Indexer] Error reading {session_file}: {e}")

        # Also check OpenClaw's own session storage
        openclaw_sessions = os.path.join(
            os.path.expanduser("~"), ".openclaw", "sessions"
        )
        if os.path.exists(openclaw_sessions):
            for session_file in Path(openclaw_sessions).glob("*.json"):
                if session_file.stem in self.index_state.get("indexed_sessions", []):
                    continue  # Already indexed

                try:
                    with open(session_file, 'r', encoding='utf-8') as f:
                        session_data = json.load(f)

                    mtime = datetime.fromtimestamp(os.path.getmtime(session_file))

                    if (datetime.now() - mtime).total_seconds() / 3600 <= hours_back:
                        sessions.append({
                            "id": session_file.stem,
                            "path": str(session_file),
                            "last_modified": mtime.isoformat(),
                            "data": session_data
                        })
                except Exception as e:
                    print(f"[Indexer] Error reading {session_file}: {e}")

        return sessions
    
    def parse_session_messages(self, session_data: Dict) -> List[Dict]:
        """
        Parse session data into individual messages.
        Returns list of message dicts with metadata.
        """
        messages = []
        
        # Handle various session formats
        if "messages" in session_data:
            msg_list = session_data["messages"]
        elif isinstance(session_data, list):
            msg_list = session_data
        else:
            # Try to find messages in unknown format
            return []
        
        for i, msg in enumerate(msg_list):
            if isinstance(msg, dict):
                parsed = {
                    "id": msg.get("id", f"msg_{i}"),
                    "role": msg.get("role", "unknown"),
                    "content": msg.get("content", ""),
                    "timestamp": msg.get("timestamp", msg.get("created_at", "")),
                    "channel": msg.get("channel", session_data.get("channel", "unknown")),
                    "session_id": session_data.get("id", "unknown")
                }
                
                # Handle nested content
                if isinstance(parsed["content"], list):
                    parsed["content"] = " ".join([
                        c.get("text", "") if isinstance(c, dict) else str(c)
                        for c in parsed["content"]
                    ])
                elif not isinstance(parsed["content"], str):
                    parsed["content"] = str(parsed["content"])
                
                messages.append(parsed)
        
        return messages
    
    def score_message_importance(self, message: Dict) -> float:
        """
        Score a message's importance (0.0 to 1.0).
        
        Factors:
        - Has user confirmed something important
        - Contains a decision or commitment
        - Contains new user information
        - Message from assistant with successful outcome
        """
        content = message.get("content", "").lower()
        role = message.get("role", "")
        
        importance = 0.5  # Base importance
        
        # High importance indicators
        if any(kw in content for kw in [
            "remember", "important", "don't forget", "remind me",
            "my name is", "i'm working on", "preference", "always"
        ]):
            importance += 0.2
        
        # Decision/commitment indicators
        if any(kw in content for kw in [
            "decided", "going to", "will use", "should",
            "plan is", "let's go with"
        ]):
            importance += 0.15
        
        # Success confirmation
        if any(kw in content for kw in ["perfect", "thanks", "great", "works", "awesome"]):
            importance += 0.1
        
        # User messages are slightly more important
        if role == "user":
            importance += 0.05
        
        # Very long messages might be context-rich
        if len(content) > 500:
            importance += 0.05
        
        # Cap at 1.0
        return min(1.0, importance)
    
    def extract_tags(self, message: Dict) -> List[str]:
        """Extract tags from message content."""
        content = message.get("content", "").lower()
        tags = []
        
        # Topic tags
        topic_keywords = {
            "coding": ["code", "function", "script", "debug", "api", "python", "javascript"],
            "ai": ["ai", "model", "llm", "gpt", "hermes", "agent"],
            "project": ["project", "building", "creating", "working on"],
            "help": ["help", "how to", "can you", "need to"],
            "question": ["what", "why", "how", "when", "where", "?"],
            "memory": ["remember", "forget", "recall", "remind"],
            "tool": ["search", "browse", "run", "execute", "create"]
        }
        
        for topic, keywords in topic_keywords.items():
            if any(kw in content for kw in keywords):
                tags.append(topic)
        
        # Channel tag
        channel = message.get("channel", "unknown")
        if channel != "unknown":
            tags.append(f"channel:{channel}")
        
        return tags
    
    def index_session(self, session: Dict) -> int:
        """
        Index a single session's messages into the vector DB.
        Returns number of messages indexed.

        Accepts adapter-shaped sessions (with pre-parsed "messages") or the
        legacy shape (raw document in "data", parsed via
        parse_session_messages).
        """
        session_id = session["id"]
        agent_key = self._agent_key()
        state_key = f"{agent_key}:{session_id}"

        # Skip if already indexed (dedup by agent + session id). This
        # per-session short-circuit stays FIRST: content-hash dedup only
        # applies to messages from sessions not yet marked indexed.
        if state_key in self.index_state.get("indexed_sessions", []):
            return 0

        messages = session.get("messages")
        if not messages:
            # Legacy shape: raw session document
            messages = self.parse_session_messages(session.get("data", {}))
            for msg in messages:
                msg.setdefault("session_id", session_id)

        hash_set = set(self.index_state.get("content_hashes", []))
        indexed_count = 0
        duplicates_skipped = 0
        kept_messages = []
        for msg in messages:
            content = msg.get("content")
            if not content or len(content) < 10:
                continue  # Skip very short/empty messages

            content_hash = self._content_hash(content)
            if content_hash in hash_set:
                # Same normalized text already indexed in ANY session —
                # store once, skip here.
                duplicates_skipped += 1
                self.last_duplicates_skipped += 1
                continue

            importance = self.score_message_importance(msg)
            tags = self.extract_tags(msg)

            self.vector_db.add_memory(
                content=content,
                session_id=session_id,
                importance=importance,
                tags=tags,
                metadata={
                    "role": msg.get("role"),
                    "channel": msg.get("channel"),
                    "timestamp": msg.get("timestamp"),
                    "source_agent": self.adapter.get_agent_name(),
                    "source_file": session.get("path") or None,
                }
            )
            hash_set.add(content_hash)
            self.index_state.setdefault("content_hashes", []).append(content_hash)
            indexed_count += 1
            kept_messages.append(msg)

        # Mark as indexed when the session produced work (new rows OR
        # fully-duplicate content) so future scans do not reprocess it.
        if indexed_count > 0 or duplicates_skipped > 0:
            self.index_state["indexed_sessions"].append(state_key)
            self.index_state["total_messages_indexed"] += indexed_count
        if indexed_count > 0:
            self.last_new_session_messages[session_id] = kept_messages

        return indexed_count

    def run_indexing(self, hours_back: int = 24) -> Dict[str, Any]:
        """
        Run full indexing cycle on recent sessions.
        Returns indexing report.
        """
        report = {
            "sessions_found": 0,
            "sessions_indexed": 0,
            "messages_indexed": 0,
            "duplicates_skipped": 0,
            "newly_indexed_sessions": [],
            "adapter": self.adapter.get_agent_name(),
            "errors": []
        }

        self.last_new_session_messages = {}
        self.last_duplicates_skipped = 0

        sessions = self.get_sessions(hours_back=hours_back)
        report["sessions_found"] = len(sessions)

        for session in sessions:
            try:
                count = self.index_session(session)
                if count > 0:
                    report["sessions_indexed"] += 1
                    report["messages_indexed"] += count
                    report["newly_indexed_sessions"].append(session["id"])
            except Exception as e:
                report["errors"].append({
                    "session_id": session.get("id"),
                    "error": str(e)
                })

        report["duplicates_skipped"] = self.last_duplicates_skipped

        self.index_state["last_adapter"] = report["adapter"]
        self._save_index_state()

        return report
    
    def reindex_all(self) -> Dict[str, Any]:
        """
        Re-index all sessions from scratch.
        WARNING: Resets index state (including content hashes, so a full
        rebuild actually re-adds everything once).
        """
        self.index_state = self._fresh_index_state()
        self._save_index_state()

        return self.run_indexing(hours_back=24 * 30)  # Last 30 days

    def get_stats(self) -> Dict:
        """Get indexing statistics."""
        return {
            "adapter": self.adapter.get_agent_name(),
            "index_state": {
                "total_indexed": len(self.index_state.get("indexed_sessions", [])),
                "total_messages": self.index_state.get("total_messages_indexed", 0),
                "last_run": self.index_state.get("last_index_run")
            },
            "vector_db_stats": self.vector_db.get_stats()
        }
