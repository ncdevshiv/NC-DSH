"""
LanceMem Self-Optimizer - Matrix-Based Pruning with LanceDB.
Performance optimization using vector-based entity scoring.
"""

import os
import json
import logging
import numpy as np
from typing import Dict, List, Any, Optional, Tuple
from datetime import datetime, timedelta
from collections import defaultdict

from memory_store.vector_db import get_vector_db, LanceDBVectorStore

logger = logging.getLogger("openmem.optimizer")


class LanceDBOptimizer:
    """
    LanceDB-backed performance optimization system.
    
    Uses LanceDB to store and query:
    - Entity performance scores
    - Entity connections (relationships)
    - Optimization history
    - Pruning candidates
    
    Advantages over SQLite:
    - Sub-millisecond queries on millions of entities
    - Automatic indexing on all columns
    - Time-travel queries (versioning)
    - Cloud-native backup/restore
    """

    ENTITY_TABLE = "entity_scores"
    CONNECTIONS_TABLE = "entity_connections"
    HISTORY_TABLE = "optimization_history"

    def __init__(self, base_path: str = None):
        self.base_path = base_path or os.path.join(os.path.dirname(__file__), "..", "data", "optimizer")
        os.makedirs(self.base_path, exist_ok=True)

        # Use centralized data/ directory for LanceDB
        self.db_path = os.path.join(self.base_path, "optimizer_lancedb")
        self.db = None
        self._init_db()

        # Pruning thresholds
        self.prune_threshold = 0.2
        self.strengthen_threshold = 0.7
        self.prune_age_days = 30

    def _init_db(self):
        """Initialize LanceDB tables."""
        try:
            import lancedb
            self.db = lancedb.connect(self.db_path)
            self._create_tables()
            print(f"[LanceDBOptimizer] Connected to {self.db_path}")
        except Exception as e:
            print(f"[LanceDBOptimizer] Failed to init LanceDB: {e}")
            self.db = None

    def _create_tables(self):
        """Create optimization tables."""
        if self.db is None:
            return

        try:
            table_names = self.db.table_names()

            # Entity scores table
            if self.ENTITY_TABLE not in table_names:
                entity_schema = """
                    id: string,
                    entity_id: string,
                    entity_type: string,
                    usage_count: int,
                    success_count: int,
                    failure_count: int,
                    last_used: string,
                    created_at: string,
                    performance_score: float,
                    prune_score: float,
                    metadata: string
                """
                self.db.create_table(self.ENTITY_TABLE, data=[])
                print(f"[LanceDBOptimizer] Created table: {self.ENTITY_TABLE}")

            # Entity connections table
            if self.CONNECTIONS_TABLE not in table_names:
                self.db.create_table(self.CONNECTIONS_TABLE, data=[])
                print(f"[LanceDBOptimizer] Created table: {self.CONNECTIONS_TABLE}")

            # Optimization history table
            if self.HISTORY_TABLE not in table_names:
                self.db.create_table(self.HISTORY_TABLE, data=[])
                print(f"[LanceDBOptimizer] Created table: {self.HISTORY_TABLE}")

        except Exception as e:
            print(f"[LanceDBOptimizer] Table creation failed: {e}")

    def _generate_id(self, entity_id: str, id_type: str) -> str:
        """Generate unique record ID."""
        return f"{id_type}_{entity_id}_{datetime.now().strftime('%H%M%S')}"

    def register_entity(
        self,
        entity_id: str,
        entity_type: str,
        metadata: Dict = None
    ) -> bool:
        """Register a new entity in the performance matrix."""
        if self.db is None:
            print("[LanceDBOptimizer] DB not available")
            return False

        try:
            table = self.db.open_table(self.ENTITY_TABLE)
            
            # Check if exists
            existing = table.search(f'entity_id = "{entity_id}"').limit(1).to_list()
            if existing:
                return False

            # Add new entity
            record = {
                "id": self._generate_id(entity_id, "entity"),
                "entity_id": entity_id,
                "entity_type": entity_type,
                "usage_count": 0,
                "success_count": 0,
                "failure_count": 0,
                "last_used": datetime.now().isoformat(),
                "created_at": datetime.now().isoformat(),
                "performance_score": 0.5,
                "prune_score": 1.0,
                "metadata": json.dumps(metadata or {})
            }

            table.add([record])
            return True
        except Exception as e:
            print(f"[LanceDBOptimizer] Register failed: {e}")
            return False

    def record_usage(
        self,
        entity_id: str,
        success: bool,
        context: Dict = None
    ):
        """Record entity usage and update performance score."""
        if self.db is None:
            return

        try:
            table = self.db.open_table(self.ENTITY_TABLE)
            
            # Find entity
            results = table.search(f'entity_id = "{entity_id}"').limit(1).to_list()
            if not results:
                return

            entity = results[0]
            
            # Update counts
            usage_count = entity["usage_count"] + 1
            success_count = entity["success_count"] + (1 if success else 0)
            failure_count = entity["failure_count"] + (0 if success else 1)

            # Calculate performance score
            total = success_count + failure_count
            perf_score = success_count / total if total > 0 else 0.5

            # Calculate prune score with recency
            last_used = entity.get("last_used", datetime.now().isoformat())
            days_since_use = (datetime.now() - datetime.fromisoformat(last_used)).days
            recency_factor = max(0.1, 1.0 - (days_since_use / 60))
            prune_score = (1.0 - perf_score) * recency_factor

            # Update
            table.update(
                where=f'entity_id = "{entity_id}"',
                values={
                    "usage_count": usage_count,
                    "success_count": success_count,
                    "failure_count": failure_count,
                    "performance_score": perf_score,
                    "prune_score": prune_score,
                    "last_used": datetime.now().isoformat()
                }
            )

            # Log to history
            self._log_optimization(
                "use",
                entity_id,
                entity["performance_score"],
                perf_score,
                f"Success: {success}"
            )

        except Exception as e:
            print(f"[LanceDBOptimizer] Record usage failed: {e}")

    def _log_optimization(
        self,
        action: str,
        entity_id: str,
        old_score: float,
        new_score: float,
        reason: str
    ):
        """Log an optimization action."""
        if self.db is None:
            return

        try:
            table = self.db.open_table(self.HISTORY_TABLE)
            
            record = {
                "id": self._generate_id(entity_id, action),
                "action": action,
                "entity_id": entity_id,
                "old_score": old_score,
                "new_score": new_score,
                "reason": reason,
                "timestamp": datetime.now().isoformat()
            }
            
            table.add([record])
        except Exception as e:
            print(f"[LanceDBOptimizer] Log failed: {e}")

    def get_all_entities(self, entity_type: str = None) -> List[Dict]:
        """Get all entities with their scores."""
        if self.db is None:
            return []

        try:
            table = self.db.open_table(self.ENTITY_TABLE)
            results = table.to_list()
            
            if entity_type:
                results = [r for r in results if r.get("entity_type") == entity_type]
            
            return results
        except Exception as e:
            print(f"[LanceDBOptimizer] Get all failed: {e}")
            return []

    def get_entities_by_score(self, min_score: float = 0.0, entity_type: str = None) -> List[Dict]:
        """Get entities filtered by minimum performance score."""
        if self.db is None:
            return []

        try:
            table = self.db.open_table(self.ENTITY_TABLE)
            
            # Use LanceDB's SQL-like filtering
            results = table.search(f"performance_score >= {min_score}").limit(1000).to_list()
            
            if entity_type:
                results = [r for r in results if r.get("entity_type") == entity_type]
            
            return results
        except Exception as e:
            print(f"[LanceDBOptimizer] Score filter failed: {e}")
            return []

    def calculate_prune_candidates(self, entity_type: str = None) -> List[Tuple]:
        """Calculate entities that should be pruned."""
        if self.db is None:
            return []

        try:
            table = self.db.open_table(self.ENTITY_TABLE)
            
            # Get entities above prune threshold
            results = table.search(f"prune_score > {self.prune_threshold}").limit(100).to_list()
            
            if entity_type:
                results = [r for r in results if r.get("entity_type") == entity_type]

            candidates = []
            for r in results:
                days_since_use = 0
                if r.get("last_used"):
                    days_since_use = (datetime.now() - datetime.fromisoformat(r["last_used"])).days

                should_prune = (
                    r["prune_score"] > self.prune_threshold or
                    (days_since_use > self.prune_age_days and r["performance_score"] < 0.5)
                )

                if should_prune:
                    candidates.append((
                        r["entity_id"],
                        r["prune_score"],
                        r["performance_score"],
                        r["usage_count"],
                        days_since_use
                    ))

            # Sort by prune score descending
            candidates.sort(key=lambda x: x[1], reverse=True)
            return candidates

        except Exception as e:
            print(f"[LanceDBOptimizer] Prune candidates failed: {e}")
            return []

    def prune_entity(self, entity_id: str, reason: str = None) -> bool:
        """Prune (deactivate) an entity."""
        if self.db is None:
            return False

        try:
            table = self.db.open_table(self.ENTITY_TABLE)
            
            # Get current score
            results = table.search(f'entity_id = "{entity_id}"').limit(1).to_list()
            if not results:
                return False

            old_score = results[0]["performance_score"]

            # Mark as pruned (high prune score, zero perf)
            table.update(
                where=f'entity_id = "{entity_id}"',
                values={
                    "prune_score": 2.0,
                    "performance_score": 0.0
                }
            )

            self._log_optimization(
                "prune",
                entity_id,
                old_score,
                0.0,
                reason or "Prune threshold exceeded"
            )

            return True
        except Exception as e:
            print(f"[LanceDBOptimizer] Prune failed: {e}")
            return False

    def strengthen_entity(self, entity_id: str, boost: float = 0.1) -> bool:
        """Strengthen an entity's performance."""
        if self.db is None:
            return False

        try:
            table = self.db.open_table(self.ENTITY_TABLE)
            
            results = table.search(f'entity_id = "{entity_id}"').limit(1).to_list()
            if not results:
                return False

            old_score = results[0]["performance_score"]
            new_score = min(1.0, old_score + boost)

            table.update(
                where=f'entity_id = "{entity_id}"',
                values={"performance_score": new_score}
            )

            self._log_optimization(
                "strengthen",
                entity_id,
                old_score,
                new_score,
                f"Boosted by {boost}"
            )

            return True
        except Exception as e:
            print(f"[LanceDBOptimizer] Strengthen failed: {e}")
            return False

    def build_performance_matrix(self, entity_type: str = None) -> np.ndarray:
        """Build NxN performance matrix from entity scores."""
        entities = self.get_all_entities(entity_type)
        
        if not entities:
            return np.array([])

        n = len(entities)
        entity_ids = [e["entity_id"] for e in entities]
        
        # Build matrix with performance scores
        matrix = np.zeros((n, n))
        
        for i, entity in enumerate(entities):
            perf = entity["performance_score"]
            matrix[i][i] = perf  # Diagonal = self-performance
            
            # Off-diagonal could represent connections (simplified)
            # In production, use entity_connections table

        return matrix

    def run_optimization_cycle(self) -> Dict:
        """Run a complete optimization cycle."""
        report = {
            "started_at": datetime.now().isoformat(),
            "pruned": [],
            "strengthened": [],
            "analyzed": 0,
            "matrix_shape": 0
        }

        # Analyze entities
        entities = self.get_all_entities()
        report["analyzed"] = len(entities)

        # Get prune candidates
        candidates = self.calculate_prune_candidates()

        # Prune bottom performers
        prune_count = max(1, len(candidates) // 5)
        for entity_id, prune_score, perf_score, usage, days in candidates[-prune_count:]:
            if self.prune_entity(entity_id, f"Prune: {prune_score:.3f}, Perf: {perf_score:.2f}"):
                report["pruned"].append({
                    "entity_id": entity_id,
                    "prune_score": prune_score,
                    "perf_score": perf_score
                })

        # Strengthen top performers
        top_performers = self.get_entities_by_score(min_score=self.strengthen_threshold)
        strengthen_count = max(1, len(top_performers) // 3)

        for entity in top_performers[:strengthen_count]:
            if self.strengthen_entity(entity["entity_id"], boost=0.05):
                report["strengthened"].append({
                    "entity_id": entity["entity_id"],
                    "old_score": entity["performance_score"],
                    "new_score": min(1.0, entity["performance_score"] + 0.05)
                })

        # Build matrix
        matrix = self.build_performance_matrix()
        report["matrix_shape"] = matrix.shape[0]

        return report

    def get_top_performers(self, entity_type: str = None, limit: int = 10) -> List[Dict]:
        """Get top performing entities."""
        return self.get_entities_by_score(min_score=0.6, entity_type=entity_type)[:limit]

    def get_optimization_history(self, limit: int = 50) -> List[Dict]:
        """Get recent optimization history."""
        if self.db is None:
            return []

        try:
            table = self.db.open_table(self.HISTORY_TABLE)
            results = table.search("true").limit(limit).to_list()
            return sorted(results, key=lambda x: x.get("timestamp", ""), reverse=True)
        except Exception as e:
            logger.debug(f"Failed to get optimization history: {e}")
            return []

    def get_stats(self) -> Dict:
        """Get optimizer statistics."""
        if self.db is None:
            return {"error": "DB not available"}

        try:
            table = self.db.open_table(self.ENTITY_TABLE)
            entities = table.to_list()

            by_type = defaultdict(lambda: {"count": 0, "avg_perf": 0, "avg_prune": 0})
            
            for e in entities:
                etype = e.get("entity_type", "unknown")
                by_type[etype]["count"] += 1
                by_type[etype]["avg_perf"] += e.get("performance_score", 0)
                by_type[etype]["avg_prune"] += e.get("prune_score", 0)

            # Average
            for etype in by_type:
                count = by_type[etype]["count"]
                if count > 0:
                    by_type[etype]["avg_perf"] /= count
                    by_type[etype]["avg_prune"] /= count

            matrix = self.build_performance_matrix()

            return {
                "total_entities": len(entities),
                "by_type": dict(by_type),
                "prune_threshold": self.prune_threshold,
                "strengthen_threshold": self.strengthen_threshold,
                "matrix_size": matrix.shape[0]
            }
        except Exception as e:
            return {"error": str(e)}


# Alias for backwards compatibility
MatrixPruner = LanceDBOptimizer

# Singleton
_optimizer = None

def get_optimizer() -> LanceDBOptimizer:
    """Get singleton optimizer instance."""
    global _optimizer
    if _optimizer is None:
        _optimizer = LanceDBOptimizer()
    return _optimizer
