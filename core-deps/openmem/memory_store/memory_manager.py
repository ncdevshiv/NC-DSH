"""
Memory Manager for OpenMem.
Handles memory consolidation: daily → weekly → long-term memory distillation.
"""

import os
import json
import hashlib
import logging
import sqlite3
from pathlib import Path
from datetime import datetime, timedelta
from typing import Dict, List, Any, Optional
from collections import defaultdict

from . import get_vector_db

logger = logging.getLogger(__name__)


class MemoryManager:
    """
    Consolidated memory management with automatic distillation.
    
    Memory tiers:
    - Daily memory (raw events, logs)
    - Weekly summary (condensed from daily)
    - Long-term memory (important distilled facts)
    """
    
    def __init__(self, base_path: str = None):
        # Use centralized data/ directory
        if base_path is None:
            base_path = os.path.join(os.path.dirname(__file__), "..", "data", "memory")
        self.base_path = os.path.abspath(base_path)
        os.makedirs(self.base_path, exist_ok=True)

        # Paths for different memory tiers
        self.daily_path = os.path.join(self.base_path, "daily")
        self.weekly_path = os.path.join(self.base_path, "weekly")
        self.longterm_path = os.path.join(self.base_path, "longterm")

        os.makedirs(self.daily_path, exist_ok=True)
        os.makedirs(self.weekly_path, exist_ok=True)
        os.makedirs(self.longterm_path, exist_ok=True)
        
        self.vector_db = get_vector_db()
        
        # SQLite for memory metadata
        self.db_path = os.path.join(self.base_path, "memory_meta.db")
        self._init_db()
    
    def _init_db(self):
        """Initialize metadata database."""
        self.conn = sqlite3.connect(self.db_path)
        self.conn.execute("""
            CREATE TABLE IF NOT EXISTS memory_tiers (
                id TEXT PRIMARY KEY,
                tier TEXT NOT NULL,
                date_key TEXT,
                content TEXT,
                created_at TEXT,
                importance REAL DEFAULT 0.5,
                source_memories TEXT
            )
        """)
        self.conn.execute("""
            CREATE TABLE IF NOT EXISTS distillation_log (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                source_tier TEXT,
                target_tier TEXT,
                source_keys TEXT,
                distilled_at TEXT,
                summary TEXT
            )
        """)
        self.conn.commit()
    
    @staticmethod
    def _content_hash(content: str) -> str:
        """Content digest used for stable ids (matches vector_db.py id style)."""
        return hashlib.sha256(content.encode("utf-8")).hexdigest()[:16]

    def _row_content(self, memory_id: str) -> Optional[str]:
        """Return stored content for a memory id, or None if absent."""
        cursor = self.conn.execute(
            "SELECT content FROM memory_tiers WHERE id = ?", (memory_id,)
        )
        row = cursor.fetchone()
        return row[0] if row else None

    def store_daily_memory(self, date: str, content: str, source_memories: List[str] = None) -> str:
        """
        Store a daily memory entry.
        date format: YYYY-MM-DD

        The id is a stable content hash, so re-storing identical content is
        idempotent (no duplicate rows or vector entries).
        """
        memory_id = f"daily_{date}_{self._content_hash(content)}"

        # Skip when an identical entry already exists: keeps re-indexing and
        # repeated consolidation runs from duplicating vector-store entries.
        if self._row_content(memory_id) == content:
            logger.debug(f"[MemoryManager] Daily memory {memory_id} unchanged; skipping re-store")
            return memory_id

        self.conn.execute("""
            INSERT OR REPLACE INTO memory_tiers (id, tier, date_key, content, created_at, source_memories)
            VALUES (?, ?, ?, ?, ?, ?)
        """, (
            memory_id,
            "daily",
            date,
            content,
            datetime.now().isoformat(),
            json.dumps(source_memories or [])
        ))
        self.conn.commit()

        # Also add to vector DB
        self.vector_db.add_memory(
            content=content,
            importance=0.6,
            tags=["daily", date],
            metadata={"tier": "daily", "date": date, "memory_id": memory_id}
        )

        return memory_id

    def store_weekly_summary(self, week_start: str, content: str, source_daily_keys: List[str] = None) -> str:
        """
        Store a weekly summary.

        The id is a stable content hash, so re-distilling an identical summary
        is idempotent. source_daily_keys should carry the REAL daily-memory ids
        the summary was distilled from (used as lineage).
        """
        memory_id = f"weekly_{week_start}_{self._content_hash(content)}"

        # Skip when an identical summary already exists: makes consolidation
        # re-runs non-duplicating in both SQLite and the vector store.
        if self._row_content(memory_id) == content:
            logger.debug(f"[MemoryManager] Weekly summary {memory_id} unchanged; skipping re-store")
            return memory_id

        self.conn.execute("""
            INSERT OR REPLACE INTO memory_tiers (id, tier, date_key, content, created_at, source_memories)
            VALUES (?, ?, ?, ?, ?, ?)
        """, (
            memory_id,
            "weekly",
            week_start,
            content,
            datetime.now().isoformat(),
            json.dumps(source_daily_keys or [])
        ))
        self.conn.commit()

        # Add to vector DB with higher importance for summaries
        self.vector_db.add_memory(
            content=content,
            importance=0.7,
            tags=["weekly", week_start],
            metadata={"tier": "weekly", "week": week_start, "memory_id": memory_id}
        )

        return memory_id

    def store_longterm_memory(self, key: str, content: str, confidence: float = 0.8) -> str:
        """
        Store a long-term memory (important distilled fact).

        Idempotent: storing an unchanged key/content pair again neither
        rewrites the row nor adds a duplicate vector entry.
        """
        memory_id = f"longterm_{key}"

        # Skip when an identical entry already exists
        if self._row_content(memory_id) == content:
            logger.debug(f"[MemoryManager] Long-term memory {memory_id} unchanged; skipping re-store")
            return memory_id

        self.conn.execute("""
            INSERT OR REPLACE INTO memory_tiers (id, tier, date_key, content, created_at, importance)
            VALUES (?, ?, ?, ?, ?, ?)
        """, (
            memory_id,
            "longterm",
            key,
            content,
            datetime.now().isoformat(),
            confidence
        ))
        self.conn.commit()

        # Long-term memories get highest importance in vector DB
        self.vector_db.add_memory(
            content=f"{key}: {content}",
            importance=0.9,
            tags=["longterm", "important", key],
            metadata={"tier": "longterm", "key": key, "memory_id": memory_id}
        )

        return memory_id
    
    def get_daily_memories(self, date: str) -> List[Dict]:
        """Get all daily memories for a specific date."""
        cursor = self.conn.execute("""
            SELECT id, content, created_at, importance, source_memories
            FROM memory_tiers
            WHERE tier = 'daily' AND date_key = ?
            ORDER BY created_at DESC
        """, (date,))
        
        results = []
        for row in cursor.fetchall():
            results.append({
                "id": row[0],
                "content": row[1],
                "created_at": row[2],
                "importance": row[3],
                "source_memories": json.loads(row[4])
            })
        return results
    
    def get_weekly_summaries(self, weeks_back: int = 4) -> List[Dict]:
        """Get weekly summaries from the last N weeks."""
        cursor = self.conn.execute("""
            SELECT id, date_key, content, created_at, importance
            FROM memory_tiers
            WHERE tier = 'weekly'
            ORDER BY date_key DESC
            LIMIT ?
        """, (weeks_back,))
        
        results = []
        for row in cursor.fetchall():
            results.append({
                "id": row[0],
                "week_start": row[1],
                "content": row[2],
                "created_at": row[3],
                "importance": row[4]
            })
        return results
    
    def get_longterm_memories(self, key_pattern: str = None) -> List[Dict]:
        """Get long-term memories, optionally filtered by key pattern."""
        if key_pattern:
            cursor = self.conn.execute("""
                SELECT id, date_key, content, created_at, importance
                FROM memory_tiers
                WHERE tier = 'longterm' AND date_key LIKE ?
                ORDER BY importance DESC, created_at DESC
            """, (f"%{key_pattern}%",))
        else:
            cursor = self.conn.execute("""
                SELECT id, date_key, content, created_at, importance
                FROM memory_tiers
                WHERE tier = 'longterm'
                ORDER BY importance DESC, created_at DESC
            """)
        
        results = []
        for row in cursor.fetchall():
            results.append({
                "id": row[0],
                "key": row[1],
                "content": row[2],
                "created_at": row[3],
                "importance": row[4]
            })
        return results
    
    def distill_daily_to_weekly(self, week_start: str) -> Optional[str]:
        """
        Consolidate daily memories from a week into a weekly summary.
        Uses LLM-based summarization when available, falls back to heuristic.
        Returns the weekly summary ID.

        Idempotent: if a weekly summary already exists for week_start it is
        reused untouched. The summary's source_memories lineage carries the
        REAL ids of the daily memories it was distilled from.
        """
        # Reuse an existing summary for this week instead of re-distilling
        existing = self.conn.execute(
            "SELECT id FROM memory_tiers WHERE tier = 'weekly' AND date_key = ?",
            (week_start,)
        ).fetchone()
        if existing:
            logger.debug(
                f"[MemoryManager] Weekly summary for {week_start} already exists "
                f"({existing[0]}); skipping re-distillation"
            )
            return existing[0]

        # Get all daily memories for this week, WITH their real ids for lineage
        cursor = self.conn.execute("""
            SELECT id, content, importance FROM memory_tiers
            WHERE tier = 'daily' AND date_key >= ? AND date_key < ?
            ORDER BY importance DESC, created_at DESC
        """, (week_start, self._next_week(week_start)))

        daily_memories = cursor.fetchall()
        if not daily_memories:
            return None

        # Try LLM-based summarization
        try:
            from core.llm import get_llm
        except ImportError:
            # Expected degradation when the optional LLM module is missing
            logger.info("[MemoryManager] core.llm not available; using heuristic weekly summary")
            summary = self._heuristic_weekly_summary(week_start, daily_memories)
        else:
            try:
                llm = get_llm()
                if llm.is_available:
                    combined = "\n".join(f"- {m[1]}" for m in daily_memories[:20])
                    summary = llm.summarize(
                        f"Week of {week_start} daily memories:\n{combined}",
                        max_length=500
                    )
                else:
                    # Heuristic fallback
                    summary = self._heuristic_weekly_summary(week_start, daily_memories)
            except Exception as e:
                # Unexpected failure: log with traceback before falling back
                logger.exception(
                    f"[MemoryManager] LLM weekly summarization failed ({e}); using heuristic fallback"
                )
                summary = self._heuristic_weekly_summary(week_start, daily_memories)

        # Lineage: carry the REAL daily-memory ids through (previously these
        # were fabricated placeholders like daily_{week}_{i})
        daily_ids = [row[0] for row in daily_memories]
        return self.store_weekly_summary(week_start, summary, daily_ids)

    def _heuristic_weekly_summary(self, week_start: str, daily_memories: list) -> str:
        """Create weekly summary without LLM.

        Rows are (id, content, importance) tuples as selected by
        distill_daily_to_weekly.
        """
        high_priority = [m[1] for m in daily_memories if m[2] >= 0.6]
        if high_priority:
            return f"Week of {week_start}: " + "; ".join(high_priority[:5])
        return f"Week of {week_start}: " + "; ".join([m[1] for m in daily_memories[:3]])
    
    def distill_weekly_to_longterm(self, weeks_back: int = 4) -> List[str]:
        """
        Extract important facts from weekly summaries into long-term memory.
        Returns list of long-term memory keys created.
        """
        summaries = self.get_weekly_summaries(weeks_back)
        created = []
        
        for summary in summaries:
            content = summary["content"]
            
            # Simple extraction: look for patterns that suggest important facts
            # In production, this would use NER + LLM
            if any(kw in content.lower() for kw in ["preference", "always", "never", "important", "remember"]):
                # Extract as long-term
                key = f"from_week_{summary['week_start']}"
                # Skip (and don't report as created) when already distilled
                if self._row_content(f"longterm_{key}") == content:
                    continue
                self.store_longterm_memory(key, content, confidence=0.7)
                created.append(key)
        
        return created
    
    def search_memory(self, query: str, n_results: int = 5) -> List[Dict]:
        """Search across all memory tiers using vector DB."""
        return self.vector_db.search(query, n_results=n_results)
    
    def get_memory_context(self, query: str, hours: int = 168) -> str:
        """
        Get relevant memory context for a query.
        Returns formatted string for injection into context.
        """
        results = self.search_memory(query, n_results=5)
        
        if not results:
            return ""
        
        context_parts = ["## Relevant Memory Context\n"]
        for r in results:
            tier = r.get("metadata", {}).get("tier", "unknown")
            content = r["content"]
            context_parts.append(f"**[{tier}]** {content}\n")
        
        return "\n".join(context_parts)
    
    def _next_week(self, week_start: str) -> str:
        """Get the start of the next week after week_start."""
        dt = datetime.strptime(week_start, "%Y-%m-%d")
        dt += timedelta(days=7)
        return dt.strftime("%Y-%m-%d")
    
    def run_consolidation(self, dry_run: bool = False) -> Dict[str, Any]:
        """
        Run full memory consolidation:
        1. Daily → Weekly (for completed weeks)
        2. Weekly → Long-term (for recent weeks)
        
        Returns consolidation report.
        """
        report = {
            "daily_processed": 0,
            "weekly_created": 0,
            "longterm_created": 0,
            "actions": []
        }
        
        today = datetime.now()
        
        # Find the most recent Sunday (start of week)
        days_since_sunday = today.weekday() + 1
        most_recent_sunday = (today - timedelta(days=days_since_sunday)).strftime("%Y-%m-%d")
        
        # Distill the previous week (if it exists)
        prev_week_start = (datetime.strptime(most_recent_sunday, "%Y-%m-%d") - timedelta(days=7)).strftime("%Y-%m-%d")
        
        if not dry_run:
            # Report honestly: only count a weekly summary we actually created;
            # an already-distilled week is logged as reused, not created.
            existing_weekly = self.conn.execute(
                "SELECT id FROM memory_tiers WHERE tier = 'weekly' AND date_key = ?",
                (prev_week_start,)
            ).fetchone()
            if existing_weekly:
                logger.info(
                    f"[MemoryManager] Weekly summary for {prev_week_start} already "
                    f"present ({existing_weekly[0]}); skipping re-distillation"
                )
            else:
                weekly_id = self.distill_daily_to_weekly(prev_week_start)
                if weekly_id:
                    report["weekly_created"] += 1
                    report["actions"].append(f"Created weekly summary: {weekly_id}")
        
        # Extract long-term from recent weeks
        longterm_keys = self.distill_weekly_to_longterm(weeks_back=4)
        report["longterm_created"] = len(longterm_keys)
        report["actions"].extend([f"Created long-term: {k}" for k in longterm_keys])
        
        return report
    
    def get_stats(self) -> Dict[str, Any]:
        """Get memory system statistics."""
        cursor = self.conn.execute("""
            SELECT tier, COUNT(*), AVG(importance) 
            FROM memory_tiers 
            GROUP BY tier
        """)
        
        tier_stats = {}
        for row in cursor.fetchall():
            tier_stats[row[0]] = {"count": row[1], "avg_importance": row[2] or 0}
        
        return {
            "tiers": tier_stats,
            "vector_db_stats": self.vector_db.get_stats()
        }
    
    def close(self):
        """Close database connections."""
        self.conn.close()
        self.vector_db.close()
